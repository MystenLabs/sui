use proc_macro::TokenStream;
use std::iter;

use itertools::Itertools;
use proc_macro2::{Group, Span, TokenStream as TokenStream2};
use proc_macro2::{Ident, TokenTree};
use quote::quote;
use syn::parse::{Parse, ParseStream, Parser};
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::{
    parse, Attribute, GenericArgument, LitStr, PatType, Path, PathArguments, Token, TraitItem, Type,
};

#[proc_macro_attribute]
pub fn open_rpc(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut trait_data: syn::ItemTrait = syn::parse(item).unwrap();
    let rpc = parse_rpc_method(&mut trait_data).unwrap();
    let open_rpc_name = quote::format_ident!("{}OpenRpc", &rpc.name);
    let mut methods = Vec::new();
    for method in rpc.methods {
        let name = method.name;
        let doc = method.doc;
        let mut inputs = Vec::new();
        for (name, ty) in method.params {
            let (ty, required) = extract_type_from_option(ty);
            inputs.push(quote! {
                let des = builder.create_content_descriptor::<#ty>(#name, "", "", #required);
                inputs.push(des);
            })
        }
        let returns_ty = if let Some(ty) = method.returns {
            let (ty, required) = extract_type_from_option(ty);
            let name = quote! {#ty}.to_string();
            quote! {Some(builder.create_content_descriptor::<#ty>(#name, "", "", #required));}
        } else {
            quote! {None;}
        };
        methods.push(quote! {
            let mut inputs: Vec<open_rpc::ContentDescriptor> = Vec::new();
            #(#inputs)*
            let result = #returns_ty
            builder.add_method(#name, inputs, result, #doc);
        })
    }

    quote! {
        #trait_data
        pub struct #open_rpc_name;
        impl #open_rpc_name {
            pub fn open_rpc(proj_name:&str, namespace:&str) -> open_rpc::Project{
                let mut builder = open_rpc::ProjectBuilder::new(proj_name, namespace);
                #(#methods)*
                builder.build()
            }
        }
    }
    .into()
}

struct RpcDefinition {
    name: Ident,
    methods: Vec<Method>,
}
struct Method {
    name: String,
    params: Vec<(String, Type)>,
    returns: Option<Type>,
    doc: String,
}

fn parse_rpc_method(trait_data: &mut syn::ItemTrait) -> Result<RpcDefinition, syn::Error> {
    let mut methods = Vec::new();
    for trait_item in &mut trait_data.items {
        if let TraitItem::Method(method) = trait_item {
            let method_name = if let Some(attr) = find_attr(&method.attrs, "method").cloned() {
                let arguments: Punctuated<Argument, Token![,]> =
                    parenthesized.parse2(attr.tokens)?;
                arguments
                    .into_iter()
                    .find(|arg| arg.label == "name")
                    .unwrap()
            } else {
                panic!("-1")
            };

            let doc = extract_doc_comments(&method.attrs).to_string();

            let params: Vec<_> = method
                .sig
                .inputs
                .iter_mut()
                .filter_map(|arg| match arg {
                    syn::FnArg::Receiver(_) => None,
                    syn::FnArg::Typed(arg) => match *arg.pat.clone() {
                        syn::Pat::Ident(name) => {
                            Some(get_type(arg).map(|ty| (name.ident.to_string(), ty)))
                        }
                        syn::Pat::Wild(wild) => Some(Err(syn::Error::new(
                            wild.underscore_token.span(),
                            "Method argument names must be valid Rust identifiers; got `_` instead",
                        ))),
                        _ => Some(Err(syn::Error::new(
                            arg.span(),
                            format!("Unexpected method signature input; got {:?} ", *arg.pat),
                        ))),
                    },
                })
                .collect::<Result<_, _>>()?;

            let returns = match &method.sig.output {
                syn::ReturnType::Default => None,
                syn::ReturnType::Type(_, output) => extract_type_from(&*output, "RpcResult"),
            };

            methods.push(Method {
                name: method_name.string()?,
                params,
                returns,
                doc,
            });
        }
    }
    Ok(RpcDefinition {
        name: trait_data.ident.clone(),
        methods,
    })
}

fn extract_type_from(ty: &Type, from_ty: &str) -> Option<Type> {
    fn path_is(path: &Path, from_ty: &str) -> bool {
        path.leading_colon.is_none()
            && path.segments.len() == 1
            && path.segments.iter().next().unwrap().ident == from_ty
    }

    if let Type::Path(p) = ty {
        if p.qself.is_none() && path_is(&p.path, from_ty) {
            if let PathArguments::AngleBracketed(a) = &p.path.segments[0].arguments {
                if let Some(GenericArgument::Type(ty)) = a.args.first() {
                    return Some(ty.clone());
                }
            }
        }
    }
    None
}

fn extract_type_from_option(ty: Type) -> (Type, bool) {
    if let Some(ty) = extract_type_from(&ty, "Option") {
        (ty, false)
    } else {
        (ty, true)
    }
}

fn get_type(pat_type: &mut PatType) -> Result<Type, syn::Error> {
    Ok(
        if let Some((pos, attr)) = pat_type
            .attrs
            .iter()
            .find_position(|a| a.path.is_ident("schemars"))
        {
            let arguments: Punctuated<Argument, Token![,]> =
                parenthesized.parse2(attr.tokens.clone())?;
            let arg = arguments
                .into_iter()
                .find(|arg| arg.label == "with")
                .unwrap();
            let path = parse_lit_str(&arg.value::<LitStr>()?)?;
            pat_type.attrs.remove(pos);
            path
        } else {
            pat_type.ty.as_ref().clone()
        },
    )
}

#[derive(Debug)]
struct Argument {
    pub label: syn::Ident,
    pub tokens: TokenStream2,
}
impl Parse for Argument {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let label = input.parse()?;

        let mut scope = 0usize;

        // Need to read to till either the end of the stream,
        // or the nearest comma token that's not contained
        // inside angle brackets.
        let tokens = iter::from_fn(move || {
            if scope == 0 && input.peek(Token![,]) {
                return None;
            }

            if input.peek(Token![<]) {
                scope += 1;
            } else if input.peek(Token![>]) {
                scope = scope.saturating_sub(1);
            }

            input.parse::<TokenTree>().ok()
        })
        .collect();

        Ok(Argument { label, tokens })
    }
}

impl Argument {
    pub fn value<T: Parse>(self) -> syn::Result<T> {
        fn value_parser<T: Parse>(stream: ParseStream) -> syn::Result<T> {
            stream.parse::<Token![=]>()?;
            stream.parse()
        }

        value_parser.parse2(self.tokens)
    }

    /// Asserts that the argument is `key = "string"` and gets the value of the string
    pub fn string(self) -> syn::Result<String> {
        self.value::<LitStr>().map(|lit| lit.value())
    }
}

fn parenthesized<T: Parse>(input: ParseStream) -> syn::Result<Punctuated<T, Token![,]>> {
    let content;
    syn::parenthesized!(content in input);
    content.parse_terminated(T::parse)
}

fn find_attr<'a>(attrs: &'a [Attribute], ident: &str) -> Option<&'a Attribute> {
    attrs.iter().find(|a| a.path.is_ident(ident))
}

fn parse_lit_str<T>(s: &syn::LitStr) -> parse::Result<T>
where
    T: Parse,
{
    let tokens = spanned_tokens(s)?;
    syn::parse2(tokens)
}

fn spanned_tokens(s: &syn::LitStr) -> parse::Result<TokenStream2> {
    let stream = syn::parse_str(&s.value())?;
    Ok(respan_token_stream(stream, s.span()))
}

fn respan_token_stream(stream: TokenStream2, span: Span) -> TokenStream2 {
    stream
        .into_iter()
        .map(|token| respan_token_tree(token, span))
        .collect()
}

fn respan_token_tree(mut token: TokenTree, span: Span) -> TokenTree {
    if let TokenTree::Group(g) = &mut token {
        *g = Group::new(g.delimiter(), respan_token_stream(g.stream(), span));
    }
    token.set_span(span);
    token
}

fn extract_doc_comments(attrs: &[syn::Attribute]) -> String {
    attrs
        .iter()
        .filter(|attr| {
            attr.path.is_ident("doc")
                && match attr.parse_meta() {
                    Ok(syn::Meta::NameValue(meta)) => matches!(&meta.lit, syn::Lit::Str(_)),
                    _ => false,
                }
        })
        .map(|attr| {
            let s = attr.tokens.to_string();
            s[4..s.len() - 1].to_string()
        })
        .join(" ")
}
