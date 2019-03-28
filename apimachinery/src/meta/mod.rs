use std::convert::From;
use std::fmt;
use std::result::Result;

pub mod v1;

// GroupVersionKind unambiguously identifies a kind.
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

// GroupVersion contains the "group" and the "version", which uniquely
// identifies the API.
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

// GroupKind specifies a Group and a Kind, but does not force a
// version.  This is useful for identifying concepts during lookup
// stages without having partially valid types.
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

// GroupVersionResource unambiguously identifies a resource.
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

// GroupResource specifies a Group and a Resource, but does not force
// a version.  This is useful for identifying concepts during lookup
// stages without having partially valid types.
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
