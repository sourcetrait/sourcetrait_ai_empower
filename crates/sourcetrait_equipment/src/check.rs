use crate::*;

pub struct StrCheck {
    case: StrCase,
    min_len: usize,
    max_len: usize,
}

impl StrCheck {
    pub const DEFAULT: Self = Self {
        case: StrCase::None,
        min_len: 1,
        max_len: usize::MAX, 
    };
}

impl Default for StrCheck {
    fn default() -> Self { Self::DEFAULT }
}

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq)]
pub enum StrCase {
    #[default]
    None,
    Snake,
}

impl StrCase {
    pub fn check(&self, s: &str) -> bool {
        match self {
            Self::None => true,
            Self::Snake => Self::is_snake_case(s),
        }
    }
    
    fn is_snake_case(s: &str) -> bool {
        for c in s.chars() {
            match c {
                'a'..'z' => (),
                '0'..'9' => (),
                '_' => (),
                _ => return false,
            }
        }

        !s.contains("__") && !s.starts_with('_') && !s.ends_with('_')
    }
}

impl StrCheck {
    pub const fn case(mut self, case: StrCase) -> Self {
        self.case = case;
        self
    }

    pub const fn max_len(mut self, len: usize) -> Self {
        self.max_len = len;
        self
    }
    
    pub const fn min_len(mut self, len: usize) -> Self {
        self.min_len = len;
        self
    }

    pub fn check(self, s: &str) -> EquipmentResult<()> {
        let len = s.len();
        if len < self.min_len {
            Err(EquipmentError::String { str: s.to_string(), err: StrErr::MinLen(self.min_len) })
        } else if len > self.max_len {
            Err(EquipmentError::String { str: s.to_string(), err: StrErr::MaxLen(self.max_len) })
        } else if !self.case.check(&s) {
            Err(EquipmentError::String { str: s.to_string(), err: StrErr::Case(self.case) })
        } else {
            Ok(())
        }
    }

    pub fn valid(self, s: String) -> EquipmentResult<String> {
        self.check(&s)?;
        Ok(s)
    }
}
