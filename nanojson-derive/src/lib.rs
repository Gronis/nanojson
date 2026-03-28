extern crate proc_macro;

use proc_macro::TokenStream;

mod helpers;
mod parse_item;
mod codegen;

#[proc_macro_derive(Serialize, attributes(nanojson))]
pub fn derive_serialize(input: TokenStream) -> TokenStream {
    match parse_item::parse_item(input) {
        Ok(item) => match codegen::gen_serialize(&item) {
            Ok(ts) => ts,
            Err(err) => err,
        },
        Err(err) => err,
    }
}

#[proc_macro_derive(Deserialize, attributes(nanojson))]
pub fn derive_deserialize(input: TokenStream) -> TokenStream {
    match parse_item::parse_item(input) {
        Ok(item) => match codegen::gen_deserialize(&item) {
            Ok(ts) => ts,
            Err(err) => err,
        },
        Err(err) => err,
    }
}
