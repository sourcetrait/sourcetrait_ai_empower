use crate::*;

#[cereal::derived(Data)]
pub enum ValueData {
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Glob(GlobData),
    Filesize(i64),
    Duration(i64),
    Date(DateData),
    Range(RangeData),
    Record(Vec<(String, Box<ValueData>)>),
    List(Vec<ValueData>),
    Error,
    Binary(Vec<u8>),
    CellPath,
    Nothing,
}

#[cereal::derived(Data)]
pub struct GlobData {
    pub glob: String,
    pub no_expand: bool,
}

/// chrono::DateTime<FixedOffset>
#[cereal::derived(Data)]
pub struct DateData {
    pub seconds: u32,
    pub seconds_fraction: u32,
    pub utc_offset_minus: i32,
    pub date_yof: NonZero<i32>,
}

#[cereal::derived(Data)]
pub enum RangeData {
    Int(TypedRangeData<u64>),
    Float(TypedRangeData<f64>),
}

#[cereal::derived(Data)]
#[serde(bound(
    serialize = "T: cereal::DataCopy",
    deserialize = "T: cereal::DataCopy"
))]
pub struct TypedRangeData<T> {
    pub start: f64,
    pub step: f64,
    pub end: Bounded<T>,
}

#[cereal::derived(Data, Copy)]
#[serde(bound(
    serialize = "T: cereal::DataCopy",
    deserialize = "T: cereal::DataCopy"
))]
pub enum Bounded<T> {
    Included(T),
    Excluded(T),
    Unbounded,
}

#[cereal::derived(Data)]
pub struct SpanData {
    pub start: u64,
    pub end: u64,
}

#[cereal::derived(Data)]
pub struct ErrorData {
    pub code: String,
    pub msg: String,
    pub span: Option<SpanData>,
}

/*
pub enum Value {
    #[non_exhaustive]
    Bool {
        val: bool,
        /// note: spans are being refactored out of Value
        /// please use .span() instead of matching this span value
        #[serde(rename = "span")]
        internal_span: Span,
    },
    #[non_exhaustive]
    Int {
        val: i64,
        /// note: spans are being refactored out of Value
        /// please use .span() instead of matching this span value
        #[serde(rename = "span")]
        internal_span: Span,
    },
    #[non_exhaustive]
    Float {
        val: f64,
        /// note: spans are being refactored out of Value
        /// please use .span() instead of matching this span value
        #[serde(rename = "span")]
        internal_span: Span,
    },
    #[non_exhaustive]
    String {
        val: String,
        /// note: spans are being refactored out of Value
        /// please use .span() instead of matching this span value
        #[serde(rename = "span")]
        internal_span: Span,
    },
    #[non_exhaustive]
    Glob {
        val: String,
        no_expand: bool,
        /// note: spans are being refactored out of Value
        /// please use .span() instead of matching this span value
        #[serde(rename = "span")]
        internal_span: Span,
    },
    #[non_exhaustive]
    Filesize {
        val: Filesize,
        /// note: spans are being refactored out of Value
        /// please use .span() instead of matching this span value
        #[serde(rename = "span")]
        internal_span: Span,
    },
    #[non_exhaustive]
    Duration {
        /// The duration in nanoseconds.
        val: i64,
        /// note: spans are being refactored out of Value
        /// please use .span() instead of matching this span value
        #[serde(rename = "span")]
        internal_span: Span,
    },
    #[non_exhaustive]
    Date {
        val: DateTime<FixedOffset>,
        /// note: spans are being refactored out of Value
        /// please use .span() instead of matching this span value
        #[serde(rename = "span")]
        internal_span: Span,
    },
    #[non_exhaustive]
    Range {
        val: Box<Range>,
        #[serde(skip)]
        signals: Option<Signals>,
        /// note: spans are being refactored out of Value
        /// please use .span() instead of matching this span value
        #[serde(rename = "span")]
        internal_span: Span,
    },
    #[non_exhaustive]
    Record {
        val: SharedCow<Record>,
        /// note: spans are being refactored out of Value
        /// please use .span() instead of matching this span value
        #[serde(rename = "span")]
        internal_span: Span,
    },
    #[non_exhaustive]
    List {
        vals: Vec<Value>,
        #[serde(skip)]
        signals: Option<Signals>,
        /// note: spans are being refactored out of Value
        /// please use .span() instead of matching this span value
        #[serde(rename = "span")]
        internal_span: Span,
    },
    #[non_exhaustive]
    Closure {
        val: Box<Closure>,
        /// note: spans are being refactored out of Value
        /// please use .span() instead of matching this span value
        #[serde(rename = "span")]
        internal_span: Span,
    },
    #[non_exhaustive]
    Error {
        error: Box<ShellError>,
        /// note: spans are being refactored out of Value
        /// please use .span() instead of matching this span value
        #[serde(rename = "span")]
        internal_span: Span,
    },
    #[non_exhaustive]
    Binary {
        val: Vec<u8>,
        /// note: spans are being refactored out of Value
        /// please use .span() instead of matching this span value
        #[serde(rename = "span")]
        internal_span: Span,
    },
    #[non_exhaustive]
    CellPath {
        val: CellPath,
        /// note: spans are being refactored out of Value
        /// please use .span() instead of matching this span value
        #[serde(rename = "span")]
        internal_span: Span,
    },
    #[non_exhaustive]
    Custom {
        val: Box<dyn CustomValue>,
        /// note: spans are being refactored out of Value
        /// please use .span() instead of matching this span value
        #[serde(rename = "span")]
        internal_span: Span,
    },
    #[non_exhaustive]
    Nothing {
        /// note: spans are being refactored out of Value
        /// please use .span() instead of matching this span value
        #[serde(rename = "span")]
        internal_span: Span,
    },
}
*/