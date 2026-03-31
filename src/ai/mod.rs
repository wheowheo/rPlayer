// AI analysis — deferred, trait definitions only

/// A decoded video frame for AI analysis
pub struct VideoFrame {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>, // RGBA
    pub pts: f64,
}

/// Result of AI analysis to be rendered as overlay
pub enum AiOverlay {
    BoundingBoxes(Vec<BBox>),
    DepthMap { width: u32, height: u32, data: Vec<u8> },
    Skeleton(Vec<Joint>),
    FaceLandmarks(Vec<[f32; 2]>),
    HandTracking(Vec<Vec<[f32; 2]>>),
    TextRegions(Vec<TextRegion>),
    PersonMask { width: u32, height: u32, data: Vec<u8> },
    FaceSwap { width: u32, height: u32, data: Vec<u8> },
    ClothingOverlay { width: u32, height: u32, data: Vec<u8> },
}

pub struct BBox {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub label: String,
    pub confidence: f32,
}

pub struct Joint {
    pub x: f32,
    pub y: f32,
    pub confidence: f32,
}

pub struct TextRegion {
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// Trait for AI analyzers — implement this for each AI mode
pub trait AiAnalyzer: Send + 'static {
    fn name(&self) -> &str;
    fn analyze(&mut self, frame: &VideoFrame) -> Option<AiOverlay>;
}
