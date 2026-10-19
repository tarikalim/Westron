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

/// PDF text, with its fonts and styles, to DOCX, RTF or TXT; one page break per page.
/// Text only: pictures and the page layout (columns, positions) are not carried over.
pub struct PdfText(Vec<Edge>);

impl PdfText {
    pub fn new() -> Self {
        // TXT dearer, so ODT, DOC and HTML go through RTF and keep the styles.
        PdfText(vec![
            edge("pdf", "txt", 14),
            edge("pdf", "rtf", 12),
            edge("pdf", "docx", 12),
        ])
    }
}

impl Engine for PdfText {
    fn id(&self) -> &'static str {
        "pdftext"
    }

    fn edges(&self) -> &[Edge] {
        &self.0
    }

    fn convert(&self, step: &Step) -> anyhow::Result<Vec<PathBuf>> {
        let out = step.output("");
        let r = Command::new(std::env::current_exe()?)
            .arg("__pdftext")
            .arg(step.input)
            .arg(&out)
            .output()?;
        if !r.status.success() || !out.metadata().is_ok_and(|m| m.len() > 0) {
            bail!("PDF text: {}", String::from_utf8_lossy(&r.stderr).trim());
        }
        (step.progress)(1.0);
        Ok(vec![out])
    }
}

/// Runs in the child process; the format comes from `out`'s extension.
pub fn extract(input: &Path, out: &Path) -> anyhow::Result<()> {
    use objc2::runtime::AnyObject;
    use objc2_app_kit::{
        NSDocumentTypeDocumentAttribute, NSOfficeOpenXMLTextDocumentType, NSRTFTextDocumentType,
    };
    use objc2_foundation::{NSMutableAttributedString, NSRange};
    use objc2_pdf_kit::PDFDocument;

    let mtm = MainThreadMarker::new().context("not on the main thread")?;
    let _app = NSApplication::sharedApplication(mtm);
    let url = NSURL::fileURLWithPath(&NSString::from_str(&input.to_string_lossy()));
    let doc =
        unsafe { PDFDocument::initWithURL(PDFDocument::alloc(), &url) }.context("not a PDF")?;
    let text = NSMutableAttributedString::new();
    let pages = unsafe { doc.pageCount() };
    for i in 0..pages {
        if i > 0 {
            // A form feed becomes a page break in DOCX and RTF.
            text.appendAttributedString(&NSAttributedString::from_nsstring(&NSString::from_str(
                "\u{0C}",
            )));
        }
        if let Some(page) = unsafe { doc.pageAtIndex(i) }
            && let Some(s) = unsafe { page.attributedString() }
        {
            text.appendAttributedString(&s);
        }
    }
    if text.length() == 0 {
        bail!("the PDF has no text (a scan needs OCR)");
    }
    let kind = match out.extension().and_then(|e| e.to_str()) {
        Some("txt") => {
            std::fs::write(out, text.string().to_string().replace('\u{0C}', "\n\n"))?;
            return Ok(());
        }
        Some("rtf") => unsafe { NSRTFTextDocumentType },
        _ => unsafe { NSOfficeOpenXMLTextDocumentType },
    };
    let attrs = NSDictionary::<NSString, AnyObject>::from_slices(
        &[unsafe { NSDocumentTypeDocumentAttribute }],
        &[kind.as_ref()],
    );
    let data = unsafe {
        text.dataFromRange_documentAttributes_error(NSRange::new(0, text.length()), &attrs)
    }
    .map_err(|e| anyhow!("{}", e.localizedDescription()))?;
    std::fs::write(out, data.to_vec())?;
    Ok(())
}
