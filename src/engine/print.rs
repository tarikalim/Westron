// Rich text to PDF with macOS's text system: AppKit reads the file, the print system lays out A4 pages.
// AppKit wants the main thread, so each conversion runs in a child process (`westron __pdf`).

use super::{Edge, Engine, Step, edge};
use anyhow::{Context, anyhow, bail};
use objc2::{AnyThread as _, MainThreadMarker, MainThreadOnly as _};
use objc2_app_kit::{
    NSApplication, NSAttributedStringDocumentFormats as _, NSPrintInfo, NSPrintJobDisposition,
    NSPrintJobSavingURL, NSPrintOperation, NSPrintSaveJob, NSPrintingPaginationMode, NSTextView,
};
use objc2_foundation::{
    NSAttributedString, NSDictionary, NSPoint, NSRect, NSSize, NSString, NSURL,
};
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct RichPdf(Vec<Edge>);

impl RichPdf {
    pub fn new() -> Self {
        let mut e: Vec<Edge> = ["html", "rtf", "txt"]
            .iter()
            .map(|f| edge(f, "pdf", 12))
            .collect();
        // Word files: LibreOffice (25) lays them out more faithfully when it is installed.
        e.extend(["docx", "doc", "odt"].iter().map(|f| edge(f, "pdf", 26)));
        RichPdf(e)
    }
}

impl Engine for RichPdf {
    fn id(&self) -> &'static str {
        "print"
    }

    fn edges(&self) -> &[Edge] {
        &self.0
    }

    fn convert(&self, step: &Step) -> anyhow::Result<Vec<PathBuf>> {
        let out = step.output("");
        let r = Command::new(std::env::current_exe()?)
            .arg("__pdf")
            .arg(step.input)
            .arg(&out)
            .output()?;
        if !r.status.success() || !out.metadata().is_ok_and(|m| m.len() > 0) {
            bail!("PDF: {}", String::from_utf8_lossy(&r.stderr).trim());
        }
        (step.progress)(1.0);
        Ok(vec![out])
    }
}

/// A4 in points, 2 cm margins.
const PAGE: (f64, f64) = (595.0, 842.0);
const MARGIN: f64 = 56.7;

/// Runs in the child process, on its main thread.
pub fn render(input: &Path, out: &Path) -> anyhow::Result<()> {
    let mtm = MainThreadMarker::new().context("not on the main thread")?;
    let _app = NSApplication::sharedApplication(mtm);
    let url = |p: &Path| NSURL::fileURLWithPath(&NSString::from_str(&p.to_string_lossy()));
    let text = unsafe {
        NSAttributedString::initWithURL_options_documentAttributes_error(
            NSAttributedString::alloc(),
            &url(input),
            &NSDictionary::new(),
            None,
        )
    }
    .map_err(|e| anyhow!("{}", e.localizedDescription()))?;

    let width = PAGE.0 - 2.0 * MARGIN;
    let view = NSTextView::initWithFrame(
        NSTextView::alloc(mtm),
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(width, PAGE.1)),
    );
    let storage = unsafe { view.textStorage() }.context("no text storage")?;
    storage.setAttributedString(&text);
    view.setVerticallyResizable(true);
    view.setHorizontallyResizable(false);
    view.setMaxSize(NSSize::new(width, f64::MAX));
    view.sizeToFit();

    let info = NSPrintInfo::new();
    info.setPaperSize(NSSize::new(PAGE.0, PAGE.1));
    info.setTopMargin(MARGIN);
    info.setBottomMargin(MARGIN);
    info.setLeftMargin(MARGIN);
    info.setRightMargin(MARGIN);
    info.setHorizontalPagination(NSPrintingPaginationMode::Fit);
    info.setVerticallyCentered(false);
    unsafe {
        let dict = info.dictionary();
        dict.insert(NSPrintJobDisposition, &**NSPrintSaveJob);
        dict.insert(NSPrintJobSavingURL, &*url(out));
    }
    let op = NSPrintOperation::printOperationWithView_printInfo(&view, &info);
    op.setShowsPrintPanel(false);
    op.setShowsProgressPanel(false);
    if !op.runOperation() {
        bail!("printing failed");
    }
    Ok(())
}
