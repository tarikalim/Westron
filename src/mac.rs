// AppKit pieces GPUI has no API for: the global ⌥-drag watch and the menu bar item.

use block2::RcBlock;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject};
use objc2::{ClassType as _, MainThreadMarker, MainThreadOnly as _, define_class, msg_send, sel};
use objc2_app_kit::{
    NSEvent, NSEventMask, NSEventModifierFlags, NSEventType, NSImage, NSMenu, NSMenuItem,
    NSPasteboard, NSPasteboardNameDrag, NSPasteboardTypeFileURL, NSScreen, NSStatusBar,
    NSVariableStatusItemLength,
};
use objc2_foundation::{NSString, NSURL};
use std::cell::Cell;
use std::path::PathBuf;
use std::ptr::NonNull;
use std::sync::OnceLock;

pub enum Event {
    /// Files dragged with ⌥ held; the cursor in GPUI screen coordinates (top-left origin).
    DragStart(Vec<PathBuf>, (f32, f32)),
    DragEnd,
    OpenWindow,
    Quit,
}

static SENDER: OnceLock<UnboundedSender<Event>> = OnceLock::new();

fn send(e: Event) {
    if let Some(s) = SENDER.get() {
        let _ = s.unbounded_send(e);
    }
}

/// Starts the watch and the menu bar item; events arrive on the returned channel.
pub fn start(mtm: MainThreadMarker) -> UnboundedReceiver<Event> {
    let (tx, rx) = unbounded();
    let _ = SENDER.set(tx);
    watch_drags();
    status_item(mtm);
    rx
}

/// Global monitors see other apps' events (Finder's drags) and need no permission for the mouse.
fn watch_drags() {
    let last_change = Cell::new(-1isize);
    let open = Cell::new(false);
    let block = RcBlock::new(move |event: NonNull<NSEvent>| {
        let event = unsafe { event.as_ref() };
        if event.r#type() == NSEventType::LeftMouseUp {
            if open.get() {
                open.set(false);
                send(Event::DragEnd);
            }
            return;
        }
        if open.get() || !NSEvent::modifierFlags_class().contains(NSEventModifierFlags::Option) {
            return;
        }
        let pb = NSPasteboard::pasteboardWithName(unsafe { NSPasteboardNameDrag });
        let change = pb.changeCount();
        if change == last_change.get() {
            return;
        }
        let files = dragged_files(&pb);
        if files.is_empty() {
            return;
        }
        last_change.set(change);
        open.set(true);
        send(Event::DragStart(files, cursor()));
    });
    let monitor = NSEvent::addGlobalMonitorForEventsMatchingMask_handler(
        NSEventMask::LeftMouseDragged | NSEventMask::LeftMouseUp,
        &block,
    );
    // Lives as long as the app.
    std::mem::forget(monitor);
}

fn dragged_files(pb: &NSPasteboard) -> Vec<PathBuf> {
    let Some(items) = pb.pasteboardItems() else {
        return vec![];
    };
    items
        .iter()
        .filter_map(|item| item.stringForType(unsafe { NSPasteboardTypeFileURL }))
        .filter_map(|s| NSURL::URLWithString(&s))
        .filter_map(|u| u.path().map(|p| PathBuf::from(p.to_string())))
        .collect()
}

fn cursor() -> (f32, f32) {
    let p = NSEvent::mouseLocation();
    // AppKit counts from the bottom of the primary screen.
    let h = MainThreadMarker::new()
        .and_then(|mtm| NSScreen::screens(mtm).firstObject())
        .map_or(0.0, |s| s.frame().size.height);
    (p.x as f32, (h - p.y) as f32)
}

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "WestronMenuTarget"]
    struct MenuTarget;

    impl MenuTarget {
        #[unsafe(method(open:))]
        fn open(&self, _sender: Option<&AnyObject>) {
            send(Event::OpenWindow);
        }

        #[unsafe(method(quit:))]
        fn quit(&self, _sender: Option<&AnyObject>) {
            send(Event::Quit);
        }
    }
);

fn status_item(mtm: MainThreadMarker) {
    let item = NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength);
    if let Some(button) = item.button(mtm) {
        let name = NSString::from_str("arrow.triangle.2.circlepath");
        if let Some(img) = NSImage::imageWithSystemSymbolName_accessibilityDescription(
            &name,
            Some(&NSString::from_str("Westron")),
        ) {
            img.setTemplate(true);
            button.setImage(Some(&img));
        }
    }
    let target: Retained<MenuTarget> = unsafe { msg_send![MenuTarget::class(), new] };
    let menu = NSMenu::new(mtm);
    for (title, action, key) in [
        ("Open Westron", sel!(open:), ""),
        ("Quit Westron", sel!(quit:), "q"),
    ] {
        let entry = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm),
                &NSString::from_str(title),
                Some(action),
                &NSString::from_str(key),
            )
        };
        unsafe { entry.setTarget(Some(&target)) };
        menu.addItem(&entry);
        if action == sel!(open:) {
            menu.addItem(&NSMenuItem::separatorItem(mtm));
        }
    }
    item.setMenu(Some(&menu));
    std::mem::forget((item, target));
}
