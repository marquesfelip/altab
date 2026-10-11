//! Camada de desenho: Direct2D + DirectWrite (fonte Inter embutida) + WIC para ícones.
//! Todas as coordenadas são em pixels físicos da janela (render target a 96 dpi).

use std::cell::{OnceCell, RefCell};
use std::collections::HashMap;
use std::ffi::c_void;
use std::rc::Rc;

use windows::core::{w, Interface, IUnknown};
use windows_numerics::Vector2;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::DirectWrite::*;
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Imaging::*;
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
use windows::Win32::UI::WindowsAndMessaging::HICON;

/// Cor RGBA (alfa não pré-multiplicado).
pub type Rgba = (u8, u8, u8, u8);

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum Align {
    Left,
    Right,
}

static INTER_REGULAR: &[u8] = include_bytes!("../assets/fonts/Inter-Regular.ttf");
static INTER_MEDIUM: &[u8] = include_bytes!("../assets/fonts/Inter-Medium.ttf");
static INTER_SEMIBOLD: &[u8] = include_bytes!("../assets/fonts/Inter-SemiBold.ttf");

pub struct Gfx {
    d2d: ID2D1Factory,
    dw: IDWriteFactory5,
    wic: IWICImagingFactory,
    collection: Option<IDWriteFontCollection>,
    family: windows::core::PCWSTR,
    // Precisa viver tanto quanto a coleção de fontes.
    _loader: Option<IDWriteInMemoryFontFileLoader>,
    formats: RefCell<HashMap<(i32, i32, Align), IDWriteTextFormat>>,
}

thread_local! {
    static GFX: OnceCell<Rc<Gfx>> = const { OnceCell::new() };
}

pub fn gfx() -> Rc<Gfx> {
    GFX.with(|g| g.get_or_init(|| Rc::new(Gfx::new().expect("Direct2D indisponível"))).clone())
}

impl Gfx {
    fn new() -> windows::core::Result<Self> {
        unsafe {
            let d2d: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let dw: IDWriteFactory5 = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let wic: IWICImagingFactory =
                CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;

            let (collection, loader, family) = match Self::load_inter(&dw) {
                Ok((c, l)) => (Some(c), Some(l), w!("Inter")),
                Err(_) => (None, None, w!("Segoe UI")),
            };
            Ok(Self {
                d2d,
                dw,
                wic,
                collection,
                family,
                _loader: loader,
                formats: RefCell::new(HashMap::new()),
            })
        }
    }

    unsafe fn load_inter(
        dw: &IDWriteFactory5,
    ) -> windows::core::Result<(IDWriteFontCollection, IDWriteInMemoryFontFileLoader)> {
        let loader = dw.CreateInMemoryFontFileLoader()?;
        dw.RegisterFontFileLoader(&loader)?;
        let builder = dw.CreateFontSetBuilder()?;
        let base: IDWriteFactory = dw.cast()?;
        for bytes in [INTER_REGULAR, INTER_MEDIUM, INTER_SEMIBOLD] {
            let file = loader.CreateInMemoryFontFileReference(
                &base,
                bytes.as_ptr() as *const c_void,
                bytes.len() as u32,
                None,
            )?;
            builder.AddFontFile(&file)?;
        }
        let set = builder.CreateFontSet()?;
        let coll = dw.CreateFontCollectionFromFontSet(&set)?;
        Ok((coll.cast()?, loader))
    }

    fn format(&self, px: f32, weight: i32, align: Align) -> Option<IDWriteTextFormat> {
        let key = ((px * 10.0) as i32, weight, align);
        if let Some(f) = self.formats.borrow().get(&key) {
            return Some(f.clone());
        }
        unsafe {
            let f = self
                .dw
                .CreateTextFormat(
                    self.family,
                    self.collection.as_ref(),
                    DWRITE_FONT_WEIGHT(weight),
                    DWRITE_FONT_STYLE_NORMAL,
                    DWRITE_FONT_STRETCH_NORMAL,
                    px,
                    w!("en-us"),
                )
                .ok()?;
            let _ = f.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);
            let _ = f.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);
            let _ = f.SetTextAlignment(match align {
                Align::Left => DWRITE_TEXT_ALIGNMENT_LEADING,
                Align::Right => DWRITE_TEXT_ALIGNMENT_TRAILING,
            });
            if let Ok(sign) = self.dw.CreateEllipsisTrimmingSign(&f) {
                let trim = DWRITE_TRIMMING {
                    granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER,
                    ..Default::default()
                };
                let _ = f.SetTrimming(&trim, &sign);
            }
            self.formats.borrow_mut().insert(key, f.clone());
            Some(f)
        }
    }
}

fn color(c: Rgba) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: c.0 as f32 / 255.0,
        g: c.1 as f32 / 255.0,
        b: c.2 as f32 / 255.0,
        a: c.3 as f32 / 255.0,
    }
}

fn rectf(r: RECT) -> D2D_RECT_F {
    D2D_RECT_F {
        left: r.left as f32,
        top: r.top as f32,
        right: r.right as f32,
        bottom: r.bottom as f32,
    }
}

/// Render target de uma janela, com cache dos ícones já convertidos.
pub struct Target {
    gfx: Rc<Gfx>,
    rt: ID2D1HwndRenderTarget,
    brush: ID2D1SolidColorBrush,
    icons: RefCell<HashMap<isize, ID2D1Bitmap>>,
    size: (u32, u32),
}

impl Target {
    pub fn new(hwnd: HWND, width: u32, height: u32) -> Option<Self> {
        let gfx = gfx();
        unsafe {
            let props = D2D1_RENDER_TARGET_PROPERTIES {
                r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                dpiX: 96.0,
                dpiY: 96.0,
                usage: D2D1_RENDER_TARGET_USAGE_NONE,
                minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
            };
            let hprops = D2D1_HWND_RENDER_TARGET_PROPERTIES {
                hwnd,
                pixelSize: D2D_SIZE_U { width, height },
                presentOptions: D2D1_PRESENT_OPTIONS_NONE,
            };
            let rt = gfx.d2d.CreateHwndRenderTarget(&props, &hprops).ok()?;
            rt.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
            let brush = rt.CreateSolidColorBrush(&color((0, 0, 0, 255)), None).ok()?;
            Some(Self { gfx, rt, brush, icons: RefCell::new(HashMap::new()), size: (width, height) })
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if self.size != (width, height) {
            unsafe {
                let _ = self.rt.Resize(&D2D_SIZE_U { width, height });
            }
            self.size = (width, height);
        }
    }

    /// Esquece os ícones em cache (os HICON podem mudar entre aberturas).
    pub fn clear_icons(&self) {
        self.icons.borrow_mut().clear();
    }

    pub fn begin(&self, clear: Rgba) {
        unsafe {
            self.rt.BeginDraw();
            self.rt.Clear(Some(&color(clear)));
        }
    }

    /// `false` indica que o target foi perdido e precisa ser recriado.
    pub fn end(&self) -> bool {
        unsafe { self.rt.EndDraw(None, None).is_ok() }
    }

    fn set(&self, c: Rgba) {
        unsafe { self.brush.SetColor(&color(c)) }
    }

    pub fn rrect(&self, r: RECT, radius: f32, c: Rgba) {
        self.set(c);
        let rr = D2D1_ROUNDED_RECT { rect: rectf(r), radiusX: radius, radiusY: radius };
        unsafe { self.rt.FillRoundedRectangle(&rr, &self.brush) }
    }

    pub fn circle_stroke(&self, cx: f32, cy: f32, radius: f32, width: f32, c: Rgba) {
        self.set(c);
        let e = D2D1_ELLIPSE { point: Vector2 { X: cx, Y: cy }, radiusX: radius, radiusY: radius };
        unsafe { self.rt.DrawEllipse(&e, &self.brush, width, None) }
    }

    pub fn line(&self, x0: f32, y0: f32, x1: f32, y1: f32, width: f32, c: Rgba) {
        self.set(c);
        unsafe {
            self.rt.DrawLine(
                Vector2 { X: x0, Y: y0 },
                Vector2 { X: x1, Y: y1 },
                &self.brush,
                width,
                None,
            )
        }
    }

    pub fn text(&self, s: &str, r: RECT, px: f32, weight: i32, align: Align, c: Rgba) {
        let Some(fmt) = self.gfx.format(px, weight, align) else { return };
        self.set(c);
        let wide: Vec<u16> = s.encode_utf16().collect();
        unsafe {
            self.rt.DrawText(
                &wide,
                &fmt,
                &rectf(r),
                &self.brush,
                D2D1_DRAW_TEXT_OPTIONS_CLIP,
                DWRITE_MEASURING_MODE_NATURAL,
            )
        }
    }

    /// Como `text` (alinhado à esquerda), com as posições `hits` (em `char`)
    /// em negrito e `hit_color`; `underline` sublinha as letras casadas.
    #[allow(clippy::too_many_arguments)]
    pub fn text_marked(
        &self,
        s: &str,
        r: RECT,
        px: f32,
        weight: i32,
        c: Rgba,
        hit_color: Rgba,
        underline: bool,
        hits: &[u32],
    ) {
        if hits.is_empty() {
            return self.text(s, r, px, weight, Align::Left, c);
        }
        let Some(fmt) = self.gfx.format(px, weight, Align::Left) else { return };
        let wide: Vec<u16> = s.encode_utf16().collect();
        unsafe {
            let (w, h) = ((r.right - r.left) as f32, (r.bottom - r.top) as f32);
            let Ok(layout) = self.gfx.dw.CreateTextLayout(&wide, &fmt, w, h) else { return };
            let Ok(hit_brush) = self.rt.CreateSolidColorBrush(&color(hit_color), None) else {
                return;
            };
            let effect: Option<IUnknown> = hit_brush.cast().ok();

            // Posição em `char` → faixa em unidades UTF-16.
            let mut start = 0u32;
            let mut spans = Vec::new();
            for ch in s.chars() {
                let len = ch.len_utf16() as u32;
                spans.push((start, len));
                start += len;
            }
            for &i in hits {
                let Some(&(start_position, length)) = spans.get(i as usize) else { continue };
                let range = DWRITE_TEXT_RANGE { startPosition: start_position, length };
                let _ = layout.SetFontWeight(DWRITE_FONT_WEIGHT(600), range);
                if underline {
                    let _ = layout.SetUnderline(true, range);
                }
                if let Some(e) = &effect {
                    let _ = layout.SetDrawingEffect(e, range);
                }
            }
            self.set(c);
            self.rt.DrawTextLayout(
                Vector2 { X: r.left as f32, Y: r.top as f32 },
                &layout,
                &self.brush,
                D2D1_DRAW_TEXT_OPTIONS_CLIP,
            );
        }
    }

    fn bitmap_for(&self, icon: HICON) -> Option<ID2D1Bitmap> {
        let key = icon.0 as isize;
        if let Some(b) = self.icons.borrow().get(&key) {
            return Some(b.clone());
        }
        unsafe {
            let src = self.gfx.wic.CreateBitmapFromHICON(icon).ok()?;
            let conv = self.gfx.wic.CreateFormatConverter().ok()?;
            conv.Initialize(
                &src,
                &GUID_WICPixelFormat32bppPBGRA,
                WICBitmapDitherTypeNone,
                None,
                0.0,
                WICBitmapPaletteTypeCustom,
            )
            .ok()?;
            let bmp = self.rt.CreateBitmapFromWicBitmap(&conv, None).ok()?;
            self.icons.borrow_mut().insert(key, bmp.clone());
            Some(bmp)
        }
    }

    pub fn icon(&self, icon: HICON, dest: RECT) {
        if icon.is_invalid() {
            return;
        }
        if let Some(b) = self.bitmap_for(icon) {
            unsafe {
                self.rt.DrawBitmap(
                    &b,
                    Some(&rectf(dest)),
                    1.0,
                    D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
                    None,
                )
            }
        }
    }
}
