use crate::*;

#[cereal::derived(Data)]
pub enum ToEngineSys {
    ExecuteRequest(ExecuteRequest),
}

#[cereal::derived(Data)]
pub enum FromEngineSys {
    ExecuteResponse(ExecuteResponse),
}

#[cereal::derived(Data)]
pub struct ExecuteRequest {
    args: vocab::ValueData,
}

#[cereal::derived(Data)]
pub struct ExecuteResponse {
}
