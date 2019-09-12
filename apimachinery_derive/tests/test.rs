use kubernetes_apimachinery::meta::v1::ObjectMeta;
use kubernetes_apimachinery_derive::Metadata;
use std::default::Default;

const API_GROUP: &str = "test/v1";

#[derive(Metadata)]
struct MyFoo {
    metadata: ObjectMeta,
}

#[test]
fn basic_typemeta() {
    use kubernetes_apimachinery::meta::TypeMeta;

    fn is_typemeta<T: TypeMeta>() {}
    is_typemeta::<MyFoo>();

    assert_eq!(MyFoo::api_version(), "test/v1");
    assert_eq!(MyFoo::kind(), "MyFoo");
}

#[test]
fn basic_metadata() {
    use kubernetes_apimachinery::meta::v1::Metadata;

    let foo = MyFoo {
        metadata: ObjectMeta {
            name: Some("bar".to_string()),
            ..Default::default()
        },
    };
    let obj = &foo as &dyn Metadata;
    assert_eq!(obj.api_version(), "test/v1");
    assert_eq!(obj.kind(), "MyFoo");
    assert_eq!(obj.metadata().name, Some("bar".to_string()));
}
