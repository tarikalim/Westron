// PDF pages to images, merge and split, through CoreGraphics (vector pages stay vector).

use super::{Edge, Engine, Step, edge};
use anyhow::{Context, anyhow, bail};
use objc2_core_foundation::{CFRetained, CFString, CFURL, CGPoint, CGRect, CGSize};
use objc2_core_graphics::{
    CGBitmapContextCreate, CGBitmapContextCreateImage, CGColorSpace, CGContext, CGImage,
    CGImageAlphaInfo, CGPDFBox, CGPDFContextClose, CGPDFContextCreateWithURL, CGPDFDocument,
    CGPDFPage, kCGColorSpaceSRGB,
};
use objc2_image_io::CGImageDestination;
use std::path::{Path, PathBuf};

/// Render resolution for page images, as Filee's default preset.
const DPI: f64 = 150.0;

pub struct PdfEngine {
    edges: Vec<Edge>,
}

impl PdfEngine {
    pub fn new() -> Self {
        PdfEngine {
            edges: ["png", "jpg", "tiff"]
                .into_iter()
                .map(|to| edge("pdf", to, 10))
                .collect(),
        }
    }
}

impl Engine for PdfEngine {
    fn id(&self) -> &'static str {
        "pdf"
    }

    fn edges(&self) -> &[Edge] {
        &self.edges
    }

    fn convert(&self, step: &Step) -> anyhow::Result<Vec<PathBuf>> {
        let doc = open(step.input)?;
        let n = CGPDFDocument::number_of_pages(Some(&doc));
        let uti = match step.to {
            "jpg" => "public.jpeg",
            "tiff" => "public.tiff",
            _ => "public.png",
        };
        let mut out = vec![];
        for i in 1..=n {
            let path = step.output(&if n == 1 {
                String::new()
            } else {
                format!("-{i}")
            });
            let page =
                CGPDFDocument::page(Some(&doc), i).ok_or_else(|| anyhow!("page {i} missing"))?;
            let img = render(&page)?;
            save_image(&img, &path, uti)?;
            out.push(path);
            (step.progress)(i as f32 / n as f32);
        }
        Ok(out)
    }
}

pub fn url(path: &Path) -> anyhow::Result<CFRetained<CFURL>> {
    let bytes = path.as_os_str().as_encoded_bytes();
    unsafe { CFURL::from_file_system_representation(None, bytes.as_ptr(), bytes.len() as _, false) }
        .ok_or_else(|| anyhow!("bad path {}", path.display()))
}

fn open(path: &Path) -> anyhow::Result<CFRetained<CGPDFDocument>> {
    let u = url(path)?;
    let doc = CGPDFDocument::with_url(Some(&*u))
        .with_context(|| format!("not a PDF: {}", path.display()))?;
    if !CGPDFDocument::is_unlocked(Some(&doc)) {
        bail!("{} is password protected", path.display());
    }
    Ok(doc)
}

fn render(page: &CGPDFPage) -> anyhow::Result<CFRetained<CGImage>> {
    let rect = CGPDFPage::box_rect(Some(page), CGPDFBox::CropBox);
    let turned = CGPDFPage::rotation_angle(Some(page)).rem_euclid(180) == 90;
    let (w, h) = if turned {
        (rect.size.height, rect.size.width)
    } else {
        (rect.size.width, rect.size.height)
    };
    let scale = DPI / 72.0;
    let (pw, ph) = ((w * scale).round() as usize, (h * scale).round() as usize);
    let space = CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB }));
    let ctx = unsafe {
        CGBitmapContextCreate(
            std::ptr::null_mut(),
            pw,
            ph,
            8,
            0,
            space.as_deref(),
            CGImageAlphaInfo::PremultipliedLast.0,
        )
    }
    .ok_or_else(|| anyhow!("no bitmap context for {pw}x{ph}"))?;
    let c = Some(&*ctx);
    CGContext::set_rgb_fill_color(c, 1.0, 1.0, 1.0, 1.0);
    CGContext::fill_rect(
        c,
        CGRect::new(CGPoint::ZERO, CGSize::new(pw as f64, ph as f64)),
    );
    CGContext::scale_ctm(c, scale, scale);
    let target = CGRect::new(CGPoint::ZERO, CGSize::new(w, h));
    let t = CGPDFPage::drawing_transform(Some(page), CGPDFBox::CropBox, target, 0, true);
    CGContext::concat_ctm(c, t);
    CGContext::draw_pdf_page(c, Some(page));
    CGBitmapContextCreateImage(c).ok_or_else(|| anyhow!("render failed"))
}

fn save_image(img: &CGImage, path: &Path, uti: &str) -> anyhow::Result<()> {
    let u = url(path)?;
    let dest = unsafe { CGImageDestination::with_url(&u, &CFString::from_str(uti), 1, None) }
        .ok_or_else(|| anyhow!("cannot write {}", path.display()))?;
    unsafe { dest.add_image(img, None) };
    if !unsafe { dest.finalize() } {
        bail!("cannot write {}", path.display());
    }
    Ok(())
}

/// Writes `pages` (document, 1-based page) into one new PDF.
fn write_pages(out: &Path, pages: &[(&CGPDFDocument, usize)]) -> anyhow::Result<()> {
    let u = url(out)?;
    let ctx = unsafe { CGPDFContextCreateWithURL(Some(&*u), std::ptr::null(), None) }
        .ok_or_else(|| anyhow!("cannot write {}", out.display()))?;
    let c = Some(&*ctx);
    for &(doc, i) in pages {
        let page = CGPDFDocument::page(Some(doc), i).ok_or_else(|| anyhow!("page {i} missing"))?;
        let rect = CGPDFPage::box_rect(Some(&page), CGPDFBox::MediaBox);
        unsafe { CGContext::begin_page(c, &rect) };
        CGContext::draw_pdf_page(c, Some(&page));
        CGContext::end_page(c);
    }
    CGPDFContextClose(c);
    Ok(())
}

pub fn merge(inputs: &[PathBuf], out: &Path) -> anyhow::Result<()> {
    let docs = inputs
        .iter()
        .map(|p| open(p))
        .collect::<anyhow::Result<Vec<_>>>()?;
    let pages: Vec<_> = docs
        .iter()
        .flat_map(|d| (1..=CGPDFDocument::number_of_pages(Some(d))).map(move |i| (&**d, i)))
        .collect();
    write_pages(out, &pages)
}

/// One PDF per page: `<stem>-<n>.pdf` in `dir`.
pub fn split(input: &Path, dir: &Path, stem: &str) -> anyhow::Result<Vec<PathBuf>> {
    let doc = open(input)?;
    (1..=CGPDFDocument::number_of_pages(Some(&doc)))
        .map(|i| {
            let out = dir.join(format!("{stem}-{i}.pdf"));
            write_pages(&out, &[(&doc, i)])?;
            Ok(out)
        })
        .collect()
}
