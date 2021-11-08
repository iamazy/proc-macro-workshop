extern crate proc_macro;

use proc_macro::TokenStream;
use quote::quote;
use syn::{self, spanned::Spanned};
use tracing::field::DebugValue;


#[proc_macro_derive(Builder, attributes(builder))]
pub fn derive(input: TokenStream) -> TokenStream {
    let st = syn::parse_macro_input!(input as syn::DeriveInput);
    match do_expand(&st) {
        Ok(token_stream) => token_stream.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

fn do_expand(st: &syn::DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    eprintln!("{:#?}", st.data);
    Value
    let struct_name_literal = st.ident.to_string();
    let builder_name_literal = format!("{}Builder", struct_name_literal);
    let builder_name_ident = syn::Ident::new(&builder_name_literal, st.span());

    let struct_ident = &st.ident;

    let fields = get_fields_from_derive_input(st)?;
    let builder_struct_fields_def = generate_builder_struct_fields_def(fields)?;
    let builder_struct_factory_init_clauses = generate_builder_factory_init_clauses(fields)?;
    let setters = generate_fields_setter(fields)?;
    let build_method = generate_build_method(fields, &st.ident)?;
    let ret = quote! {
      pub struct #builder_name_ident {
            #builder_struct_fields_def
      }

      impl #struct_ident {
            pub fn builder() -> #builder_name_ident {
                #builder_name_ident {
                    #(#builder_struct_factory_init_clauses),*
                }
            }
        }

      impl #builder_name_ident {
        #setters
      }

      impl #builder_name_ident {
        #build_method
      }
    };
    return Ok(ret);
}

type StructFields = syn::punctuated::Punctuated<syn::Field, syn::Token!(,)>;

fn get_fields_from_derive_input(d: &syn::DeriveInput) -> syn::Result<&StructFields> {
    if let syn::Data::Struct(syn::DataStruct {
                                 fields: syn::Fields::Named(syn::FieldsNamed { ref named, .. }),
                                 ..
                             }) = d.data
    {
        return Ok(named);
    }
    Err(syn::Error::new_spanned(
        d,
        "Must define one a Struct, not Enum".to_string(),
    ))
}

fn generate_builder_struct_fields_def(
    fields: &StructFields,
) -> syn::Result<proc_macro2::TokenStream> {
    let idents: Vec<_> = fields.iter().map(|f| &f.ident).collect();
    let types: syn::Result<Vec<_>> = fields.iter().map(|f| {
        if let Some(inner_ty) = get_generic_inner_type(&f.ty, "Option") {
            Ok(quote!(std::option::Option<#inner_ty>))
        } else if get_user_specified_ident_for_vec(f)?.is_some() {
            let origin_ty = &f.ty;
            Ok(quote!(#origin_ty))
        } else {
            let ty = &f.ty;
            Ok(quote!(std::option::Option<#ty>))
        }
    }).collect();
    let types = types?;
    let token_stream = quote! {
        #(#idents: #types),*
    };
    Ok(token_stream)
}

fn generate_builder_factory_init_clauses(
    fields: &StructFields,
) -> syn::Result<Vec<proc_macro2::TokenStream>> {
    let init_clauses: syn::Result<Vec<_>> = fields
        .iter()
        .map(|f| {
            let ident = &f.ident;
            if get_user_specified_ident_for_vec(f)?.is_some() {
                Ok(quote! {
                    #ident: std::vec::Vec::new()
                })
            } else {
                Ok(quote! {
                    #ident: std::option::Option::None
                })
            }
        })
        .collect();
    Ok(init_clauses?)
}

fn generate_fields_setter(fields: &StructFields) -> syn::Result<proc_macro2::TokenStream> {
    let idents: Vec<_> = fields.iter().map(|f| &f.ident).collect();
    let types: Vec<_> = fields.iter().map(|f| &f.ty).collect();

    let mut stream = proc_macro2::TokenStream::new();
    for (idx, (ident, ty)) in idents.iter().zip(types.iter()).enumerate() {
        let mut token_stream;
        if let Some(inner_ty) = get_generic_inner_type(ty, "Option") {
            token_stream = quote! {
                fn #ident(&mut self, #ident: #inner_ty) -> &mut Self {
                    self.#ident = std::option::Option::Some(#ident);
                    self
                }
            }
        } else if let Some(ref user_specified_ident) = get_user_specified_ident_for_vec(&fields[idx])? {
            let inner_ty = get_generic_inner_type(ty, "Vec").ok_or(syn::Error::new(fields[idx].span(), "each field must be specified with Vec field"))?;
            token_stream = quote! {
                fn #user_specified_ident(&mut self, #user_specified_ident: #inner_ty) -> &mut Self {
                    self.#ident.push(#user_specified_ident);
                    self
                }
            };
            if user_specified_ident != ident.as_ref().unwrap() {
                token_stream.extend(
                    quote! {
                        fn #ident(&mut self, #ident: #ty) -> &mut Self {
                            self.#ident = #ident.clone();
                            self
                        }
                    }
                );
            }
        }
        else {
            token_stream = quote! {
                fn #ident(&mut self, #ident: #ty) -> &mut Self {
                    self.#ident = std::option::Option::Some(#ident);
                    self
                }
            };
        }
        stream.extend(token_stream);
    }
    Ok(stream)
}

fn generate_build_method(fields: &StructFields, struct_ident: &syn::Ident) -> syn::Result<proc_macro2::TokenStream> {
    let idents: Vec<_> = fields.iter().map(|f| &f.ident).collect();
    let types: Vec<_> = fields.iter().map(|f| &f.ty).collect();

    let (mut field_streams, mut field_checkers) = (Vec::new(), Vec::new());

    for idx in 0..idents.len() {
        let ident = idents[idx];
        if get_generic_inner_type(&types[idx], "Option").is_none() && get_user_specified_ident_for_vec(&fields[idx])?.is_none() {
            let checker = quote! {
                if self.#ident.is_none() {
                    let err = format!("{} field missing", stringify!(#ident));
                    return std::result::Result::Err(err.into())
                }
            };
            field_checkers.push(checker);
        }
    }
    for idx in 0..idents.len() {
        let ident = idents[idx];
        if get_user_specified_ident_for_vec(&fields[idx])?.is_some() {
            field_streams.push(quote! {
                #ident: self.#ident.clone()
            });
        } else if get_generic_inner_type(&types[idx], "Option").is_none() {
            field_streams.push(quote! {
                #ident: self.#ident.clone().unwrap()
            });
        } else {
            let stream = quote! {
                #ident: self.#ident.clone()
            };
            field_streams.push(stream);
        }
    }
    let token_stream = quote! {
        pub fn build(&mut self) -> std::result::Result<#struct_ident, std::boxed::Box<dyn std::error::Error>> {
            #(#field_checkers)*

            let ident = #struct_ident {
                #(#field_streams),*
            };
            std::result::Result::Ok(ident)
        }
    };
    Ok(token_stream)
}

fn get_generic_inner_type<'a>(t: &'a syn::Type, outer_ident_name: &str) -> Option<&'a syn::Type> {
    if let syn::Type::Path(syn::TypePath {ref path, ..}) = t {
        if let Some(seg) = path.segments.last() {
            if seg.ident == outer_ident_name {
                if let syn::PathArguments::AngleBracketed(syn::AngleBracketedGenericArguments { ref args, .. }) = &seg.arguments {
                    if let Some(syn::GenericArgument::Type(r#type)) = args.first() {
                        return Some(r#type);
                    }
                }
            }
        }
    }
    None
}

fn get_user_specified_ident_for_vec(field: &syn::Field) -> syn::Result<Option<syn::Ident>> {
    for attr in &field.attrs {
        if let Ok(syn::Meta::List(syn::MetaList {ref path, ref nested, ..})) = attr.parse_meta() {
            if let Some(p) = path.segments.first() {
                if p.ident == "builder" {
                    if let Some(syn::NestedMeta::Meta(syn::Meta::NameValue(kv))) = nested.first() {
                        if kv.path.is_ident("each") {
                            if let syn::Lit::Str(ref ident_str) = kv.lit {
                                return Ok(Some(syn::Ident::new(ident_str.value().as_str(), attr.span())));
                            }
                        } else {
                            if let Ok(syn::Meta::List(ref list)) = attr.parse_meta() {
                                return Err(syn::Error::new_spanned(list, r#"expected `builder(each = "...")`"#))
                            }
                        }
                    }
                } else {
                    if let Ok(syn::Meta::List(ref list)) = attr.parse_meta() {
                        return Err(syn::Error::new_spanned(list, r#"expected `builder(...)`"#))
                    }
                }
            }
        }
    }
    Ok(None)
}