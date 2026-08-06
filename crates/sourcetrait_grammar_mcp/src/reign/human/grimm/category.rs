use crate::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(unused)]
pub(crate) enum GrimmCategory {
    Utility,
    Tool,
    Control,
}

impl GrimmCategory {
    const ROOT: &'static str = "grimm";
    const UTILITY: &'static str = "grimm::utility";
    const TOOL: &'static str = "grimm::tool";
    const CONTROL: &'static str = "grimm::control";
}

impl nuvocab::SignatureCategoryTrait for GrimmCategory {
    #[inline]
    fn root() -> &'static str { Self::ROOT }
    
    #[inline]
    fn str(&'static self) -> &'static str {
        match self {
            Self::Utility => Self::UTILITY,
            Self::Tool => Self::TOOL,
            Self::Control => Self::CONTROL,
        }
    }
}
