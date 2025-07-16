extern crate proc_macro;

use proc_macro::TokenStream;
use quote::quote;
use syn::{DeriveInput, parse_macro_input};

#[proc_macro_derive(PyWrapper)]
pub fn derive_pywrapper(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;
    let vis = input.vis;

    TokenStream::from(quote! {
        impl #name {
            #[inline]
            #vis fn clone_ref(&self, py: Python<'_>) -> Self {
                Self(self.0.clone_ref(py))
            }

            #[inline]
            #vis fn drop_ref(self, py: Python<'_>) {
                self.0.drop_ref(py);
            }

            #[inline]
            #vis fn into_inner(self) -> Py<PyAny> {
                self.0
            }
        }

        impl Into<Py<PyAny>> for #name {
            #[inline]
            fn into(self) -> Py<PyAny> {
                self.0
            }
        }

        impl std::ops::Deref for #name {
            type Target = Py<PyAny>;

            #[inline]
            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }

        impl AsRef<Py<PyAny>> for #name {
            #[inline]
            fn as_ref(&self) -> &Py<PyAny> {
                &self.0
            }
        }

        impl<'py> IntoPyObject<'py> for #name {
            type Target = PyAny;
            type Output = Bound<'py, Self::Target>;
            type Error = std::convert::Infallible;

            #[inline]
            fn into_pyobject(self, py: Python<'py>) -> Result<Self::Output, Self::Error> {
                Ok(self.0.into_bound(py))
            }
        }

        impl<'py> IntoPyObject<'py> for &'py #name {
            type Target = PyAny;
            type Output = Bound<'py, Self::Target>;
            type Error = std::convert::Infallible;

            #[inline]
            fn into_pyobject(self, py: Python<'py>) -> Result<Self::Output, Self::Error> {
                Ok(self.0.bind(py).clone())
            }
        }
    })
}
