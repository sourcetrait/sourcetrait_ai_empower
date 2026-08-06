use crate::*;

pub struct SignatureDef<CAT: SignatureCategoryTrait> {
    pub name: &'static str,
    pub description: &'static str,
    pub category: CAT,
}

pub trait SignatureCategoryTrait: Copy {
    fn root() -> &'static str;
    fn str(&'static self) -> &'static str;
}

pub struct ParameterDef {
    pub name: &'static str,
    pub description: &'static str,
}

pub trait SignatureTrait<CAT: SignatureCategoryTrait> {
    fn vocab(self, sigdef: &'static SignatureDef<CAT>) -> Self;
}

pub trait SignatureParameterTrait {
    fn vocab_required(self, paramdef: &'static ParameterDef, shape: nu::SyntaxShape) -> Self;
    fn vocab_optional(self, paramdef: &'static ParameterDef, shape: nu::SyntaxShape) -> Self;
}

impl<CAT: SignatureCategoryTrait> SignatureTrait<CAT> for nu::Signature {
    fn vocab(self, sigdef: &'static SignatureDef<CAT>) -> Self {
        self
            .description(sigdef.description)
            .category(nu::Category::Custom(CAT::root().into()))
            .search_terms(vec![sigdef.category.str().into()])
    }
}
    
impl SignatureParameterTrait for nu::Signature {
    fn vocab_required(self, paramdef: &'static ParameterDef, shape: nu::SyntaxShape) -> Self {
        self.required(paramdef.name, shape, paramdef.description)
    }
    
    fn vocab_optional(self, paramdef: &'static ParameterDef, shape: nu::SyntaxShape) -> Self {
        self.optional(paramdef.name, shape, paramdef.description)
    }
}
