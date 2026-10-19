pub mod nested;

pub fn validate() {}

pub struct Frame;

impl Frame {
    pub fn decode() {}
}

pub enum Kind {
    Sbus,
}

pub mod inline {
    pub fn inner() {}

    pub fn from_parent() {
        super::validate();
    }
}

#[cfg(test)]
mod tests {
    fn helper() {}

    #[test]
    fn rejects_stale() {
        helper();
    }
}
