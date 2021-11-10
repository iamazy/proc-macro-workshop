use proc_macro::TokenStream;
use quote::quote;
use syn::DeriveInput;

#[proc_macro_derive(CustomDebug, attributes(debug))]
pub fn derive(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as DeriveInput);
    match impl_debug(&input) {
        Ok(token_stream) => token_stream.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

type StructFields = syn::punctuated::Punctuated<syn::Field, syn::Token!(,)>;

fn get_fields(input: &syn::DeriveInput) -> syn::Result<&StructFields> {
    if let syn::Data::Struct(syn::DataStruct {
        fields: syn::Fields::Named(syn::FieldsNamed { ref named, .. }),
        ..
    }) = input.data
    {
        return Ok(named);
    }
    Err(syn::Error::new_spanned(
        input,
        "Must define a Struct, not Enum".to_string(),
    ))
}

fn get_field_attrs(field: &syn::Field) -> syn::Result<Option<String>> {
    for attr in &field.attrs {
        if let Ok(syn::Meta::NameValue(syn::MetaNameValue {
            ref path, ref lit, ..
        })) = attr.parse_meta()
        {
            if let Some(path) = path.segments.first() {
                if path.ident == "debug" {
                    if let syn::Lit::Str(ref ident_str) = lit {
                        return Ok(Some(ident_str.value()));
                    }
                } else {
                    if let Ok(syn::Meta::NameValue(ref name_value)) = attr.parse_meta() {
                        return Err(syn::Error::new_spanned(
                            name_value,
                            r#"expected `debug = "..."`"#,
                        ));
                    }
                }
            }
        }
    }
    Ok(None)
}

fn impl_debug(input: &syn::DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let struct_name = &input.ident;
    let fields = get_fields(&input)?;
    let idents: Vec<_> = fields
        .iter()
        .map(|f| (&f.ident, get_field_attrs(f)))
        .collect();
    let mut field_list = Vec::new();
    for idx in 0..idents.len() {
        let (ident, fmt) = &idents[idx];
        if let Some(ident) = ident {
            let field = match fmt {
                Ok(Some(fmt)) => quote! {
                    .field(stringify!(#ident), &format_args!(#fmt, &self.#ident))
                },
                _ => quote! {
                    .field(stringify!(#ident), &self.#ident)
                },
            };
            field_list.push(field);
        }
    }
    let ret = quote! {
        impl std::fmt::Debug for #struct_name {
            fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
                fmt.debug_struct(stringify!(#struct_name))
                   #(#field_list)*
                   .finish()
            }
        }
    };
    Ok(ret)
}
