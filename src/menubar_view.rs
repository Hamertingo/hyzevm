//! Preview nativo dentro do menu: um NSImageView grande no lugar do primeiro item da máquina.
//! (A API de menus do muda limita imagens a 18 pt, então a view é colocada direto no NSMenuItem.)
use objc2::rc::Retained;
use objc2::{define_class, msg_send, AllocAnyThread, DefinedClass, MainThreadOnly};
use objc2_app_kit::{NSEvent, NSImage, NSImageScaling, NSImageView, NSMenu};
use objc2_foundation::{MainThreadMarker, NSData, NSPoint, NSRect, NSSize};
use std::cell::RefCell;
use std::collections::HashMap;

pub const W: f64 = 340.0;
pub const H: f64 = 216.0;

pub struct Ivars { name: String }

define_class!(
    #[unsafe(super(NSImageView))]
    #[thread_kind = MainThreadOnly]
    #[name = "HyzePreviewView"]
    #[ivars = Ivars]
    pub struct PreviewView;

    impl PreviewView {
        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, _event: &NSEvent) {
            let name = self.ivars().name.clone();
            if let Some(item) = self.enclosingMenuItem() {
                if let Some(menu) = unsafe { item.menu() } { menu.cancelTracking(); }
            }
            std::thread::spawn(move || crate::menubar::act("open", &name));
        }

        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, _event: Option<&NSEvent>) -> bool { true }
    }
);

impl PreviewView {
    fn new(mtm: MainThreadMarker, name: &str) -> Retained<Self> {
        let this = mtm.alloc::<Self>().set_ivars(Ivars { name: name.to_string() });
        let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(W, H));
        let view: Retained<Self> = unsafe { msg_send![super(this), initWithFrame: frame] };
        view.setImageScaling(NSImageScaling::ScaleProportionallyUpOrDown);
        view
    }
}

thread_local! {
    static VIEWS: RefCell<HashMap<String, Retained<PreviewView>>> = RefCell::new(HashMap::new());
}

fn image(png: &[u8]) -> Option<Retained<NSImage>> {
    let data = NSData::with_bytes(png);
    let img = NSImage::initWithData(NSImage::alloc(), &data)?;
    img.setSize(NSSize::new(W, H)); // PNG em 2x → nítido em Retina
    Some(img)
}

/// Depois de reconstruir o menu: troca o 1º item do submenu de cada desktop ligado pela view de preview.
pub fn install(subs: &[(String, *mut std::ffi::c_void)], previews: &HashMap<String, Vec<u8>>) {
    let Some(mtm) = MainThreadMarker::new() else { return };
    VIEWS.with(|v| {
        let mut views = v.borrow_mut();
        views.clear();
        for (name, ptr) in subs {
            let menu: &NSMenu = unsafe { &*(*ptr as *const NSMenu) };
            let Some(item) = menu.itemAtIndex(0) else { continue };
            let view = PreviewView::new(mtm, name);
            if let Some(png) = previews.get(name) { if let Some(img) = image(png) { view.setImage(Some(&img)); } }
            item.setView(Some(&view));
            views.insert(name.clone(), view);
        }
    });
}

/// Novo frame da máquina (a view já instalada é atualizada no lugar, mesmo com o menu aberto).
pub fn update(name: &str, png: &[u8]) {
    VIEWS.with(|v| {
        if let Some(view) = v.borrow().get(name) { if let Some(img) = image(png) { view.setImage(Some(&img)); } }
    });
}
