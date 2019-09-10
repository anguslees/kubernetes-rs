use super::Integer;
use serde::{Deserialize, Serialize};
use std::fmt;

/// A number serialised as either an integer or a string.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(untagged)]
pub enum IntOrString {
    Int(Integer),
    String(String),
}

impl From<Integer> for IntOrString {
    fn from(i: Integer) -> Self {
        IntOrString::Int(i)
    }
}
impl From<String> for IntOrString {
    fn from(s: String) -> Self {
        Integer::from_str_radix(&s, 10)
            .map(IntOrString::Int)
            .unwrap_or(IntOrString::String(s))
    }
}

#[test]
fn intstr_parse() {
    assert_eq!(IntOrString::from(42), IntOrString::Int(42));
    assert_eq!(
        IntOrString::from("foo".to_string()),
        IntOrString::String("foo".to_string())
    );
    assert_eq!(IntOrString::from("42".to_string()), IntOrString::Int(42));
    assert_eq!(
        IntOrString::from("42Gi".to_string()),
        IntOrString::String("42Gi".to_string())
    );
}

impl PartialEq<Integer> for IntOrString {
    fn eq(&self, other: &Integer) -> bool {
        match *self {
            IntOrString::Int(i) => i == *other,
            IntOrString::String(_) => false,
        }
    }
}

impl PartialEq<str> for IntOrString {
    fn eq(&self, other: &str) -> bool {
        match *self {
            IntOrString::Int(i) => Integer::from_str_radix(other, 10) == Ok(i),
            IntOrString::String(ref s) => s == other,
        }
    }
}

impl fmt::Display for IntOrString {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            IntOrString::Int(ref i) => fmt::Display::fmt(i, f),
            IntOrString::String(ref s) => fmt::Display::fmt(s, f),
        }
    }
}
