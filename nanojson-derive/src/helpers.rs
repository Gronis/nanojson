use proc_macro::{Delimiter, Group, Ident, Literal, Punct, Spacing, Span, TokenStream, TokenTree};

pub(crate) fn emit_compilation_error(msg: &str, span: &Span) -> TokenStream {
    let span = *span;
    TokenStream::from_iter([
        TokenTree::Ident(Ident::new("compile_error", Span::call_site())),
        TokenTree::Punct({
            let mut punct = Punct::new('!', Spacing::Alone);
            punct.set_span(span);
            punct
        }),
        TokenTree::Group({
            let mut group = Group::new(Delimiter::Brace, {
                TokenStream::from_iter([TokenTree::Literal({
                    let mut string = Literal::string(msg);
                    string.set_span(span);
                    string
                })])
            });
            group.set_span(span);
            group
        }),
    ])
}

macro_rules! compiler_error {
    ( $i:expr, $($args:tt)* ) => {
        {
            #[allow(unused_imports)]
            use $crate::helpers::IntoSpanSelf;
            use $crate::helpers;
            let span = ($i).span();
            Err(helpers::emit_compilation_error(&format!($($args)*), &span))
        }
    };
    ( $($args:tt)* ) => {
        {
            use $crate::helpers;
            use proc_macro::Span;
            let span = Span::call_site();
            Err(helpers::emit_compilation_error(&format!($($args)*), &span))
        }
    };
}
pub(crate) use compiler_error;

#[allow(dead_code)]
pub(crate) trait IntoSpanSelf {
    fn span(&self) -> Span;
}

impl IntoSpanSelf for TokenTree {
    fn span(&self) -> Span { TokenTree::span(self) }
}

impl IntoSpanSelf for Ident {
    fn span(&self) -> Span { Ident::span(self) }
}

impl IntoSpanSelf for Literal {
    fn span(&self) -> Span { Literal::span(self) }
}

impl IntoSpanSelf for Punct {
    fn span(&self) -> Span { Punct::span(self) }
}

impl IntoSpanSelf for Group {
    fn span(&self) -> Span { Group::span(self) }
}
