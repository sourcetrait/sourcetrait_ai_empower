use crate::*;

#[cereal::derived(Copy, Eq, Data)]
pub enum ToEngineSys {
    Noop,
}

#[cereal::derived(Eq, Data)]
pub enum FromEngineSys {
    Noop,
}
