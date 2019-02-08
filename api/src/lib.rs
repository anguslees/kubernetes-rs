extern crate serde;
#[macro_use]
extern crate serde_derive;
#[cfg_attr(test, macro_use)]
extern crate serde_json;
#[macro_use]
extern crate failure;
#[cfg(test)]
extern crate serde_yaml;

use serde::de::{self, Deserialize, Deserializer, Unexpected};
use serde::ser::{Serialize, Serializer};
use std::borrow::Cow;
use std::fmt;
use std::marker::PhantomData;

pub mod apps;
pub mod core;
mod intstr;
pub mod meta;
pub mod unstructured;

pub type Time = String;
pub type Integer = i32;
pub use self::intstr::IntOrString;

// A fixed-point integer, serialised as a particular string format.
// See k8s.io/apimachinery/pkg/api/resource/quantity.go
// TODO: implement this with some appropriate Rust type.
pub type Quantity = String;

pub const JSON_PATCH: &'static str = "application/json-patch+json";
pub const MERGE_PATCH: &'static str = "application/merge-patch+json";
pub const STRATEGIC_MERGE_PATCH: &'static str = "application/strategic-merge-patch+json";

pub trait TypeMeta {
    fn api_version() -> &'static str;
    fn kind() -> &'static str;
}

/// Zero-sized struct that serializes to/from apiVersion/kind struct
/// based on type parameter.
#[derive(Default, Debug, Clone)]
pub struct TypeMetaImpl<T>(PhantomData<T>);

impl<T: TypeMeta> ::serde::de::Expected for TypeMetaImpl<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write!(formatter, "{}/{}", T::api_version(), T::kind())
    }
}

impl<T> PartialEq for TypeMetaImpl<T> {
    fn eq(&self, _rhs: &Self) -> bool {
        true
    }
}

/// Like TypeMetaImpl, but contains non-constant apiVersion/kind.
#[derive(Serialize, Deserialize)]
#[serde(rename = "TypeMeta", rename_all = "camelCase")]
struct TypeMetaRuntime<'a> {
    #[serde(borrow)]
    api_version: Option<Cow<'a, str>>,
    #[serde(borrow)]
    kind: Option<Cow<'a, str>>,
}

impl<T: TypeMeta> Serialize for TypeMetaImpl<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let tmp = TypeMetaRuntime {
            api_version: Some(Cow::from(T::api_version())),
            kind: Some(Cow::from(T::kind())),
        };
        tmp.serialize(serializer)
    }
}

impl<'de: 'a, 'a, T: TypeMeta> Deserialize<'de> for TypeMetaImpl<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let t = TypeMetaRuntime::deserialize(deserializer)?;
        let ret = TypeMetaImpl(PhantomData);
        match (t.api_version, t.kind) {
            (Some(a), Some(k)) => {
                if a == T::api_version() && k == T::kind() {
                    Ok(ret)
                } else {
                    let found = format!("{}/{}", a, k);
                    Err(de::Error::invalid_value(Unexpected::Other(&found), &ret))
                }
            }

            // No apiVersion/kind specified -> assume valid in context
            (None, None) => Ok(ret),

            // Partially specified -> invalid
            (Some(_), None) => Err(de::Error::missing_field("kind")),
            (None, Some(_)) => Err(de::Error::missing_field("apiVersion")),
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate serde_test;
    use self::serde_test::{assert_de_tokens, assert_de_tokens_error, assert_tokens, Token};
    use super::*;

    #[derive(Debug)]
    struct TestType;
    impl TypeMeta for TestType {
        fn api_version() -> &'static str {
            "v1alpha1"
        }
        fn kind() -> &'static str {
            "Test"
        }
    }

    #[test]
    fn test_typemeta_serde() {
        let t: TypeMetaImpl<TestType> = TypeMetaImpl(PhantomData);

        assert_tokens(
            &t,
            &[
                Token::Struct {
                    name: "TypeMeta",
                    len: 2,
                },
                Token::Str("apiVersion"),
                Token::Some,
                Token::BorrowedStr("v1alpha1"),
                Token::Str("kind"),
                Token::Some,
                Token::BorrowedStr("Test"),
                Token::StructEnd,
            ],
        );

        // Reversed order of fields
        assert_de_tokens(
            &t,
            &[
                Token::Struct {
                    name: "TypeMeta",
                    len: 2,
                },
                Token::Str("kind"),
                Token::Some,
                Token::BorrowedStr("Test"),
                Token::Str("apiVersion"),
                Token::Some,
                Token::BorrowedStr("v1alpha1"),
                Token::StructEnd,
            ],
        );

        // No apiVersion/kind is also ok
        assert_de_tokens(
            &t,
            &[
                Token::Struct {
                    name: "TypeMeta",
                    len: 0,
                },
                Token::StructEnd,
            ],
        );
    }

    #[test]
    fn test_typemeta_serde_error() {
        assert_de_tokens_error::<TypeMetaImpl<TestType>>(
            &[
                Token::Struct {
                    name: "TypeMeta",
                    len: 1,
                },
                Token::Str("kind"),
                Token::Some,
                Token::BorrowedStr("TestType"),
                Token::StructEnd,
            ],
            "missing field `apiVersion`",
        );

        assert_de_tokens_error::<TypeMetaImpl<TestType>>(
            &[
                Token::Struct {
                    name: "TypeMeta",
                    len: 1,
                },
                Token::Str("apiVersion"),
                Token::Some,
                Token::BorrowedStr("bogus"),
                Token::StructEnd,
            ],
            "missing field `kind`",
        );

        assert_de_tokens_error::<TypeMetaImpl<TestType>>(
            &[
                Token::Struct {
                    name: "TypeMeta",
                    len: 1,
                },
                Token::Str("apiVersion"),
                Token::Some,
                Token::Str("v1alpha1"),
                Token::Str("apiVersion"),
                Token::StructEnd,
            ],
            "duplicate field `apiVersion`",
        );

        assert_de_tokens_error::<TypeMetaImpl<TestType>>(
            &[
                Token::Struct {
                    name: "TypeMeta",
                    len: 2,
                },
                Token::Str("kind"),
                Token::Some,
                Token::Str("NotTest"),
                Token::Str("apiVersion"),
                Token::Some,
                Token::Str("v1alpha1"),
                Token::StructEnd,
            ],
            "invalid value: v1alpha1/NotTest, expected v1alpha1/Test",
        );
    }
}
