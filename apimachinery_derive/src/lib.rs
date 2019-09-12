extern crate proc_macro;

use proc_macro::TokenStream;
use quote::quote;
use syn;

fn impl_metadata_macro(ast: &syn::DeriveInput) -> TokenStream {
    let name = &ast.ident;
    let gen = quote! {
        impl ::kubernetes_apimachinery::meta::TypeMeta for #name {
            #[inline]
            fn api_version() -> &'static str {
                // FIXME: convenient, but not very hygienic
                API_GROUP
            }
            #[inline]
            fn kind() -> &'static str {
                stringify!(#name)
            }
        }
        impl ::kubernetes_apimachinery::meta::v1::Metadata for #name {
            #[inline]
            fn api_version(&self) -> &str {
                <Self as ::kubernetes_apimachinery::meta::TypeMeta>::api_version()
            }
            #[inline]
            fn kind(&self) -> &str {
                <Self as ::kubernetes_apimachinery::meta::TypeMeta>::kind()
            }
            #[inline]
            fn metadata(&self) -> ::std::borrow::Cow<::kubernetes_apimachinery::meta::v1::ObjectMeta> {
                ::std::borrow::Cow::Borrowed(&self.metadata)
            }
        }
    };
    gen.into()
}

#[proc_macro_derive(Metadata)]
pub fn metadata_macro_derive(input: TokenStream) -> TokenStream {
    let ast = syn::parse(input).unwrap();
    impl_metadata_macro(&ast)
}
