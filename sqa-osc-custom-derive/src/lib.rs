#![recursion_limit = "1024"]
extern crate proc_macro;
extern crate syn;
#[macro_use] extern crate quote;

use proc_macro::TokenStream;
use syn::{DeriveInput, VariantData, Body, MetaItem, Ident, Variant, Lit};

#[derive(Clone)]
enum IdentType {
    Subst,
    Ser,
    Verb(Ident),
    None
}
struct Message {
    path: String,
    ident: Ident,
    substs: Vec<Ident>,
    sers: Vec<Ident>,
    verbs: Vec<Ident>,
    alltoks: Vec<(Ident, IdentType)>
}
fn tokens_from_message(name: Ident, Message { path, substs, sers, ident, verbs, alltoks }: Message) -> (quote::Tokens, quote::Tokens) {
    let substs2 = substs.clone();
    let mut match_arm: quote::Tokens = quote::Tokens::new();
    let mut body: quote::Tokens = quote::Tokens::new();
    for chk in path.split("/").skip(1) {
        if let Some('{') = chk.chars().nth(0) {
            match_arm.append_terminated(
                &[Ident::new(chk.replace("{", "")
                           .replace("}", "")
                )],
                ","
            );
        }
        else {
            match_arm.append_terminated(
                &[chk],
                ","
            );
        }
    }
    for &(ref tok, ref typ) in alltoks.iter() {
        let tok2 = tok;
        let toks = match *typ {
            IdentType::Subst => quote! {
                let #tok = #tok2.parse()?;
            },
            IdentType::Ser => quote! {
                let #tok;
                if args.len() < 1 {
                    bail!(OSCWrongArgs("blob"))
                }
                if let Some(x) = args.remove(0).blob() {
                    #tok2 = ::rmp_serde::from_slice(&x)?;
                }
                else {
                    bail!(OSCWrongArgs("blob"))
                }
            },
            IdentType::Verb(ref id) => quote! {
                let #tok;
                if args.len() < 1 {
                    bail!(OSCWrongArgs(stringify!(#id)))
                }
                if let Some(x) = args.remove(0).#id() {
                    #tok2 = x.into();
                }
                else {
                    bail!(OSCWrongArgs(stringify!(#id)))
                }
            },
            IdentType::None => quote! {
                let #tok = ::std::default::Default::default();
            }
        };
        body.append_all(&[toks]);
    }
    let alltoks = alltoks.iter().map(|&(ref a, _)| a).collect::<Vec<_>>();
    let alltoks2 = alltoks.clone();
    let a = quote! {
        &[#match_arm] => {
            #body
            Ok(#name::#ident { #(#alltoks),* })
        },
    };
    (a, quote! {
        #name::#ident { #(#alltoks2),* } => {
            let path = format!(#path #(,#substs2=#substs)*);
            let mut args = vec![
                #(
                    OscType::Blob(
                        ::rmp_serde::to_vec(&#sers)
                            .unwrap()
                    )
                ),*
            ];
            #(
                args.push(#verbs.into());
            )*
            Some(OscMessage {
                addr: path,
                args: if args.len() == 0 {
                    None
                }
                else { Some(args) }
            })
        },
    })
}
fn message_from_variant(var: &Variant) -> Option<Message> {
    let mut path = None;
    for attr in var.attrs.iter() {
        if let MetaItem::NameValue(ref id, Lit::Str(ref st, ..)) = attr.value {
            if id == "oscpath" {
                path = Some(st.to_string());
                break;
            }
        }
    }
    let path = match path {
        Some(x) => x,
        None => return None
    };
    let mut substs = vec![];
    let mut alltoks = vec![];
    let mut sers = vec![];
    let mut verbs = vec![];
    if let VariantData::Struct(ref fields) = var.data {
        for field in fields {
            let ident = field.ident.as_ref().unwrap().clone();
            for attr in field.attrs.iter() {
                if let MetaItem::Word(ref id) = attr.value {
                    alltoks.push((ident.clone(), match &format!("{}", id) as &str {
                        "subst" => {
                            substs.push(ident.clone());
                            IdentType::Subst
                        },
                        "ser" => {
                            sers.push(ident.clone());
                            IdentType::Ser
                        },
                        "verbatim" => {
                            panic!("#[verbatim] takes an argument");
                        },
                        _ => IdentType::None
                    }));
                    break;
                }
                else if let MetaItem::NameValue(ref id, Lit::Str(ref st, ..)) = attr.value {
                    if id == "verbatim" {
                        alltoks.push((ident.clone(), IdentType::Verb(Ident::new(st as &str))));
                        verbs.push(ident.clone());
                        break;
                    }
                }
                alltoks.push((ident.clone(), IdentType::None));
            }
        }
    }
    Some(Message { path: path, substs: substs, verbs: verbs, alltoks: alltoks, sers: sers, ident:
    var.ident.clone()})
}
fn impl_ser(ast: &DeriveInput) -> quote::Tokens {
    if let Body::Enum(ref vars) = ast.body {
        let ident = ast.ident.clone();
        let msgs = vars.iter().filter_map(message_from_variant)
            .map(|m| tokens_from_message(ast.ident.clone(), m))
            .collect::<Vec<_>>();
        let mut to = vec![];
        let mut from = vec![];
        for (f, t) in msgs {
            to.push(t);
            from.push(f);
        }
        let into_impl = if to.len() != vars.len() {
            quote! {}
        }
        else {
            quote! {
                impl Into<OscMessage> for #ident {
                    fn into(self) -> OscMessage {
                        self.to_osc().unwrap()
                    }
                }
            }
        };
        let fallout_to = if to.len() != vars.len() {
            quote! {
                _ => None
            }
        }
        else {
            quote! {}
        };
        quote! {
            impl #ident {
                pub fn to_osc(self) -> Option<OscMessage> {
                    #![allow(unused_mut)]
                    match self {
                        #(#to)*
                        #fallout_to
                    }
                }
                pub fn from_osc(addr: &str, args: Option<Vec<OscType>>) -> OscResult<Self> {
                    #![allow(unused_mut)]
                    let path: Vec<&str> = (&addr).split("/").collect();
                    let mut args = if let Some(a) = args { a } else { vec![] };
                    if path.len() < 2 {
                        bail!("Blank OSC path.");
                    }
                    match &path[1..] {
                        #(#from)*
                        _ => bail!(UnknownOSCPath)
                    }
                }
            }
            impl TryFrom<OscMessage> for #ident {
                type Error = OscError;
                fn try_from(msg: OscMessage) -> OscResult<Self> {
                    Self::from_osc(&msg.addr, msg.args)
                }
            }
            #into_impl
        }
    }
    else {
        panic!("OSC impls can only be derived for enums");
    }
}
#[proc_macro_derive(OscSerde, attributes(oscpath, verbatim, subst, ser))]
pub fn osc_ser(input: TokenStream) -> TokenStream {
    let s = input.to_string();
    let ast = syn::parse_derive_input(&s).unwrap();

    let gen = impl_ser(&ast);

    gen.parse().unwrap()
}
