pub mod meta;
pub mod core;
pub mod apps;
mod intstr;

pub type Time = String;
pub type Integer = i32;
pub use self::intstr::IntOrString;

// A fixed-point integer, serialised as a particular string format.
// See k8s.io/apimachinery/pkg/api/resource/quantity.go
// TODO: implement this with some appropriate Rust type.
pub type Quantity = String;
