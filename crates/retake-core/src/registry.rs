use crate::{Renderer, Target, TargetKind};

pub struct RendererRegistry {
    renderers: Vec<Box<dyn Renderer>>,
}

impl RendererRegistry {
    pub fn new() -> Self {
        Self { renderers: vec![] }
    }

    pub fn register(&mut self, r: Box<dyn Renderer>) {
        self.renderers.push(r);
    }

    pub fn find(&self, target: &Target) -> Option<&dyn Renderer> {
        self.renderers
            .iter()
            .find(|r| self.supports(r.as_ref(), target))
            .map(|b| b.as_ref())
    }

    fn supports(&self, r: &dyn Renderer, target: &Target) -> bool {
        // simple dispatch by id matching target kind for initial
        match target.kind {
            TargetKind::Image => r.id() == "image",
            TargetKind::Text => r.id() == "text",
            TargetKind::Html | TargetKind::Web | TargetKind::Markdown => r.id() == "web",
            TargetKind::Adb => r.id() == "adb",
            TargetKind::Code => r.id() == "code",
            TargetKind::Pdf => r.id() == "pdf",
            TargetKind::Pencil => r.id() == "pencil",
            TargetKind::Video => r.id() == "video",
        }
    }
}

impl Default for RendererRegistry {
    fn default() -> Self {
        Self::new()
    }
}
