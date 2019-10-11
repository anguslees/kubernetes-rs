use crate::meta::v1::{DeleteOptions, GetOptions, ListOptions, UpdateOptions, WatchEvent};
use crate::request::Patch;
use async_trait::async_trait;
use failure::{Error, Fail};
use futures::stream::BoxStream;
use serde::de::{self, DeserializeOwned, Deserializer, Unexpected};
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::convert::From;
use std::fmt;
use std::marker::PhantomData;
use std::result::Result;

mod intstr;
pub mod v1;

pub type Time = String;
pub type Integer = i32;
pub use self::intstr::IntOrString;

// A fixed-point integer, serialised as a particular string format.
// See k8s.io/apimachinery/pkg/api/resource/quantity.go
// TODO: implement this with some appropriate Rust type.
pub type Quantity = String;

/// GroupVersionKind unambiguously identifies a kind.
#[derive(Debug, Clone, PartialEq)]
pub struct GroupVersionKind<'a> {
    pub group: &'a str,
    pub version: &'a str,
    pub kind: &'a str,
}

impl<'a> GroupVersionKind<'a> {
    // TODO: should be TryFrom, once that stabilises
    pub fn from_object<T: v1::Metadata>(m: &'a T) -> Result<Self, InvalidGroupVersionError> {
        let gv = GroupVersion::from_str(m.api_version())?;
        Ok(gv.with_kind(m.kind()))
    }

    pub fn as_gv(&self) -> GroupVersion<'a> {
        GroupVersion {
            group: self.group,
            version: self.version,
        }
    }

    pub fn as_gk(&self) -> GroupKind<'a> {
        GroupKind {
            group: self.group,
            kind: self.kind,
        }
    }
}

impl<'a> fmt::Display for GroupVersionKind<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}/{}, Kind={}", self.group, self.version, self.kind)
    }
}

impl<'a> From<GroupVersionKind<'a>> for GroupKind<'a> {
    fn from(gvk: GroupVersionKind<'a>) -> Self {
        gvk.as_gk()
    }
}

impl<'a> From<GroupVersionKind<'a>> for GroupVersion<'a> {
    fn from(gvk: GroupVersionKind<'a>) -> Self {
        gvk.as_gv()
    }
}

/// GroupVersion contains the "group" and the "version", which uniquely
/// identifies the API.
#[derive(Debug, Clone, PartialEq)]
pub struct GroupVersion<'a> {
    pub group: &'a str,
    pub version: &'a str,
}

impl<'a> GroupVersion<'a> {
    // Can't use FromStr trait because lifetimes
    pub fn from_str(s: &'a str) -> Result<Self, InvalidGroupVersionError> {
        let (g, v) = match s.find('/') {
            None => ("", s),
            Some(i) => {
                let (a, b) = s.split_at(i);
                let b = &b[1..];
                if b.find('/').is_some() {
                    return Err(InvalidGroupVersionError { value: s.into() });
                }
                (a, b)
            }
        };
        Ok(GroupVersion {
            group: g,
            version: v,
        })
    }

    pub fn with_kind(self, kind: &'a str) -> GroupVersionKind<'a> {
        GroupVersionKind {
            group: self.group,
            version: self.version,
            kind: kind,
        }
    }

    pub fn with_resource(self, rsrc: &'a str) -> GroupVersionResource<'a> {
        GroupVersionResource {
            group: self.group,
            version: self.version,
            resource: rsrc,
        }
    }

    pub fn api_prefix(&self) -> &str {
        match self {
            GroupVersion {
                group: "",
                version: "v1",
            } => "api",
            _ => "apis",
        }
    }
}

// Display puts "group" and "version" into a single "group/version"
// string. For the legacy v1 it returns "v1".
impl<'a> fmt::Display for GroupVersion<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.group == "" {
            write!(f, "{}", self.version)
        } else {
            write!(f, "{}/{}", self.group, self.version)
        }
    }
}

#[derive(Debug, Fail)]
#[fail(display = "unexpected GroupVersion string: {}", value)]
pub struct InvalidGroupVersionError {
    pub value: String,
}

#[test]
fn gv_fromstr() {
    fn gv<'a>(g: &'a str, v: &'a str) -> GroupVersion<'a> {
        GroupVersion {
            group: g,
            version: v,
        }
    }
    assert_eq!(GroupVersion::from_str("v1").unwrap(), gv("", "v1"));
    assert_eq!(GroupVersion::from_str("v2").unwrap(), gv("", "v2"));
    assert_eq!(GroupVersion::from_str("/v1").unwrap(), gv("", "v1"));
    assert_eq!(GroupVersion::from_str("v1/").unwrap(), gv("v1", ""));
    assert!(GroupVersion::from_str("/v1/").is_err());
    assert_eq!(GroupVersion::from_str("v1/a").unwrap(), gv("v1", "a"));
}

/// GroupKind specifies a Group and a Kind, but does not force a
/// version.  This is useful for identifying concepts during lookup
/// stages without having partially valid types.
#[derive(Debug, Clone, PartialEq)]
pub struct GroupKind<'a> {
    pub group: &'a str,
    pub kind: &'a str,
}

impl<'a> GroupKind<'a> {
    pub fn with_version(self, v: &'a str) -> GroupVersionKind {
        GroupVersionKind {
            group: self.group,
            version: v,
            kind: self.kind,
        }
    }
}

impl<'a> fmt::Display for GroupKind<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}.{}", self.kind, self.group)
    }
}

/// GroupVersionResource unambiguously identifies a resource.
#[derive(Debug, Clone, PartialEq)]
pub struct GroupVersionResource<'a> {
    pub group: &'a str,
    pub version: &'a str,
    pub resource: &'a str,
}

impl<'a> GroupVersionResource<'a> {
    pub fn as_gv(&self) -> GroupVersion<'a> {
        GroupVersion {
            group: self.group,
            version: self.version,
        }
    }

    pub fn as_gr(&self) -> GroupResource<'a> {
        GroupResource {
            group: self.group,
            resource: self.resource,
        }
    }
}

impl<'a> From<GroupVersionResource<'a>> for GroupResource<'a> {
    fn from(gvr: GroupVersionResource<'a>) -> Self {
        gvr.as_gr()
    }
}

impl<'a> From<GroupVersionResource<'a>> for GroupVersion<'a> {
    fn from(gvr: GroupVersionResource<'a>) -> Self {
        gvr.as_gv()
    }
}

impl<'a> fmt::Display for GroupVersionResource<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}/{}, Resource={}",
            self.group, self.version, self.resource
        )
    }
}

/// GroupResource specifies a Group and a Resource, but does not force
/// a version.  This is useful for identifying concepts during lookup
/// stages without having partially valid types.
#[derive(Debug, Clone, PartialEq)]
pub struct GroupResource<'a> {
    pub group: &'a str,
    pub resource: &'a str,
}

impl<'a> GroupResource<'a> {
    /// Turns "resource.group" string into a GroupResource struct.  Empty
    /// strings are allowed for each field.
    // Can't use FromStr trait because lifetimes
    pub fn from_str(s: &'a str) -> Result<Self, ()> {
        let (g, r) = match s.find('.') {
            None => ("", s),
            Some(i) => {
                let (a, b) = s.split_at(i);
                (&b[1..], a)
            }
        };
        Ok(GroupResource {
            group: g,
            resource: r,
        })
    }

    pub fn with_version(self, v: &'a str) -> GroupVersionResource<'a> {
        GroupVersionResource {
            group: self.group,
            version: v,
            resource: self.resource,
        }
    }
}

impl<'a> fmt::Display for GroupResource<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}.{}", self.resource, self.group)
    }
}

#[test]
fn gr_fromstr() {
    fn gr<'a>(g: &'a str, r: &'a str) -> GroupResource<'a> {
        GroupResource {
            group: g,
            resource: r,
        }
    }
    assert_eq!(GroupResource::from_str("v1").unwrap(), gr("", "v1"));
    assert_eq!(GroupResource::from_str(".v1").unwrap(), gr("v1", ""));
    assert_eq!(GroupResource::from_str("v1.").unwrap(), gr("", "v1"));
    assert_eq!(GroupResource::from_str("v1.a").unwrap(), gr("a", "v1"));
    assert_eq!(GroupResource::from_str("b.v1.a").unwrap(), gr("v1.a", "b"));
}

/// Kubernetes API type information for a static Rust type.
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
    use super::*;
    use serde_test::{assert_de_tokens, assert_de_tokens_error, assert_tokens, Token};

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

pub trait ResourceScope {
    fn url_segments(&self) -> Vec<&str>;
    fn name(&self) -> Option<&str>;
    fn namespace(&self) -> Option<&str>;
}

impl<T> ResourceScope for &T
where
    T: ResourceScope,
{
    fn url_segments(&self) -> Vec<&str> {
        (**self).url_segments()
    }
    fn name(&self) -> Option<&str> {
        (**self).name()
    }
    fn namespace(&self) -> Option<&str> {
        (**self).namespace()
    }
}

#[derive(Debug, Clone)]
pub enum NamespaceScope {
    Cluster,
    Namespace(String),
    Name { namespace: String, name: String },
}

impl ResourceScope for NamespaceScope {
    fn url_segments(&self) -> Vec<&str> {
        match self {
            Self::Cluster => vec![],
            Self::Namespace(ns) => vec!["namespace", &ns],
            Self::Name {
                namespace: ns,
                name: n,
            } => vec!["namespace", &ns, &n],
        }
    }

    fn name(&self) -> Option<&str> {
        match self {
            Self::Cluster => None,
            Self::Namespace(_) => None,
            Self::Name {
                namespace: _ns,
                name: n,
            } => Some(&n),
        }
    }

    fn namespace(&self) -> Option<&str> {
        match self {
            Self::Cluster => None,
            Self::Namespace(ns) => Some(&ns),
            Self::Name {
                namespace: ns,
                name: _n,
            } => Some(&ns),
        }
    }
}

pub enum ClusterScope {
    Cluster,
    Name(String),
}

impl ResourceScope for ClusterScope {
    fn url_segments(&self) -> Vec<&str> {
        match self {
            Self::Cluster => vec![],
            Self::Name(n) => vec![&n],
        }
    }

    fn name(&self) -> Option<&str> {
        match self {
            Self::Cluster => None,
            Self::Name(n) => Some(&n),
        }
    }

    fn namespace(&self) -> Option<&str> {
        None
    }
}

pub trait Resource {
    type Item: Serialize + DeserializeOwned + v1::Metadata + Send + 'static;
    type Scope: ResourceScope;
    type List: Serialize + DeserializeOwned + v1::List<Item = Self::Item> + Send + 'static;
    fn gvr(&self) -> GroupVersionResource;
    fn singular(&self) -> String;
    fn plural(&self) -> String {
        return self.singular() + "s";
    }
}

#[async_trait]
pub trait ResourceService {
    type Resource: Resource;

    // Note to self: match RBAC verbs, not API docs ("get", not "read"; "update" not "replace")
    async fn get(
        &self,
        name: &<Self::Resource as Resource>::Scope,
        opts: GetOptions,
    ) -> Result<<Self::Resource as Resource>::Item, Error>;
    async fn list(
        &self,
        name: &<Self::Resource as Resource>::Scope,
        opts: ListOptions,
    ) -> Result<<Self::Resource as Resource>::List, Error>;
    fn watch(
        &self,
        name: &<Self::Resource as Resource>::Scope,
        opts: ListOptions,
    ) -> BoxStream<Result<WatchEvent<<Self::Resource as Resource>::Item>, Error>>;
    async fn create(
        &self,
        value: <Self::Resource as Resource>::Item,
        opts: GetOptions,
    ) -> Result<<Self::Resource as Resource>::Item, Error>;
    async fn patch(
        &self,
        name: &<Self::Resource as Resource>::Scope,
        patch: Patch,
        opts: UpdateOptions,
    ) -> Result<<Self::Resource as Resource>::Item, Error>;
    async fn update(
        &self,
        value: <Self::Resource as Resource>::Item,
        opts: UpdateOptions,
    ) -> Result<<Self::Resource as Resource>::Item, Error>;
    async fn delete(
        &self,
        name: &<Self::Resource as Resource>::Scope,
        opts: DeleteOptions,
    ) -> Result<(), Error>;
}
