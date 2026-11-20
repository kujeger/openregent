//! openregent's derive macros: its settings declared once, on their
//! fields (`openregent::files::setting`).
//!
//! ```text
//!  #[derive(Section)]
//!  #[section(group = Interface)]                    the default group
//!  struct Ui {
//!      #[setting(name = "WASD scroll", about = "…")]
//!      wasd_scroll: bool,                           default: bool's
//!      #[setting(about = "…", default = pan::SCROLL_SPEED, range = pan::SPEEDS)]
//!      scroll_speed: i32,
//!  }
//!   ──> a static Meta a field; impl Section (visit, visit_mut); impl Default
//!
//!  #[derive(Sections)]
//!  struct Settings { ui: Ui, #[section(tab = Enhancements)] enhancements: … }
//!   ──> a static Part a section; Settings::visit, visit_mut, parts
//! ```
//!
//! A field's attributes:
//!
//! - `about`: its text, required; `""` for none.
//! - `name`: its row's name; the key in words without: `scroll_speed` is
//!   "Scroll speed".
//! - `group`: its heading; the section's without. Required but for a
//!   gated or hidden key.
//! - `default`: its value by default; its type's without.
//! - `range`: a number's range, a `RangeInclusive` of its type.
//! - `gate = Experimental | Switch`, `show = Hidden`.
//! - With `#[section(presets)]`: `classic = …`, its Classic value, or
//!   `kept`, which no preset sets; one of them, but for an experimental
//!   key, which is kept.
//!
//! The paths it writes are openregent's, so it derives there alone.

use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as Tokens};
use quote::{format_ident, quote};
use syn::{Attribute, Data, DeriveInput, Expr, Field, Fields, Ident, LitStr, parse_macro_input};

/// The attributes the derives read.
const SECTION: &str = "section";
const SETTING: &str = "setting";

/// A `#[section(…)]` on a section's struct.
#[derive(Default)]
struct Sec {
    group: Option<Ident>,
    presets: bool,
}

/// A `#[setting(…)]` on a field.
#[derive(Default)]
struct Set {
    about: Option<LitStr>,
    name: Option<LitStr>,
    group: Option<Ident>,
    default: Option<Expr>,
    range: Option<Expr>,
    classic: Option<Expr>,
    kept: bool,
    gate: Option<Ident>,
    show: Option<Ident>,
}

/// Each `#[name(…)]` of `attrs`, its values handed to `each`.
fn parsed(
    attrs: &[Attribute],
    name: &str,
    mut each: impl FnMut(syn::meta::ParseNestedMeta) -> syn::Result<()>,
) -> syn::Result<()> {
    attrs
        .iter()
        .filter(|a| a.path().is_ident(name))
        .try_for_each(|a| a.parse_nested_meta(&mut each))
}

impl Sec {
    fn of(attrs: &[Attribute]) -> syn::Result<Sec> {
        let mut sec = Sec::default();

        parsed(attrs, SECTION, |meta| {
            if meta.path.is_ident("group") {
                sec.group = Some(meta.value()?.parse()?);
            } else if meta.path.is_ident("presets") {
                sec.presets = true;
            } else {
                return Err(meta.error("expected `group` or `presets`"));
            }

            Ok(())
        })?;

        Ok(sec)
    }
}

impl Set {
    fn of(attrs: &[Attribute]) -> syn::Result<Set> {
        let mut set = Set::default();

        parsed(attrs, SETTING, |meta| {
            let path = &meta.path;

            if path.is_ident("about") {
                set.about = Some(meta.value()?.parse()?);
            } else if path.is_ident("name") {
                set.name = Some(meta.value()?.parse()?);
            } else if path.is_ident("group") {
                set.group = Some(meta.value()?.parse()?);
            } else if path.is_ident("default") {
                set.default = Some(meta.value()?.parse()?);
            } else if path.is_ident("range") {
                set.range = Some(meta.value()?.parse()?);
            } else if path.is_ident("classic") {
                set.classic = Some(meta.value()?.parse()?);
            } else if path.is_ident("kept") {
                set.kept = true;
            } else if path.is_ident("gate") {
                set.gate = Some(meta.value()?.parse()?);
            } else if path.is_ident("show") {
                set.show = Some(meta.value()?.parse()?);
            } else {
                return Err(meta.error(
                    "expected `about`, `name`, `group`, `default`, `range`, \
                     `classic`, `kept`, `gate` or `show`",
                ));
            }

            Ok(())
        })?;

        Ok(set)
    }

    /// Whether a gate other than `Always` is set.
    fn gated(&self) -> bool {
        self.gate.as_ref().is_some_and(|g| g != "Always")
    }

    /// Whether it is an experimental key.
    fn experimental(&self) -> bool {
        self.gate.as_ref().is_some_and(|g| g == "Experimental")
    }

    /// Whether the page hides it.
    fn hidden(&self) -> bool {
        self.show.as_ref().is_some_and(|s| s == "Hidden")
    }
}

/// A key in words: `scroll_speed` is "Scroll speed".
fn words(key: &str) -> String {
    let spaced = key.replace('_', " ");
    let mut chars = spaced.chars();

    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

/// `input`'s named fields.
fn named(input: &DeriveInput) -> syn::Result<Vec<&Field>> {
    match &input.data {
        Data::Struct(s) => match &s.fields {
            Fields::Named(named) => Ok(named.named.iter().collect()),
            _ => Err(syn::Error::new_spanned(input, "expected named fields")),
        },
        _ => Err(syn::Error::new_spanned(input, "expected a struct")),
    }
}

/// A field's key: its name.
fn key(field: &Field) -> String {
    field.ident.as_ref().map_or_else(String::new, |i| {
        i.to_string().trim_start_matches("r#").to_owned()
    })
}

/// The static a derive keeps its table in, named for `ident`.
fn table(ident: &Ident, what: &str) -> Ident {
    format_ident!("__{}_{}", ident.to_string().to_uppercase(), what)
}

/// A section's settings. See the crate's docs.
#[proc_macro_derive(Section, attributes(section, setting))]
pub fn section(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    expand_section(&input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// The settings' sections. See the crate's docs.
#[proc_macro_derive(Sections, attributes(section))]
pub fn sections(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    expand_sections(&input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// A field's checked attributes.
struct Checked<'a> {
    field: &'a Field,
    set: Set,
}

/// `field`'s attributes, checked against its section's `sec`.
fn checked<'a>(field: &'a Field, sec: &Sec) -> syn::Result<Checked<'a>> {
    let set = Set::of(&field.attrs)?;
    let error = |text: &str| Err(syn::Error::new_spanned(field, text));

    if field.attrs.iter().any(|a| a.path().is_ident(SECTION)) {
        error("`#[section(…)]` goes on the struct, or on a field of `Sections`")
    } else if set.about.is_none() {
        error(r#"expected `#[setting(about = "…")]`; `""` for none"#)
    } else if set.group.is_none() && sec.group.is_none() && !set.gated() && !set.hidden() {
        error("expected `group = …`: its heading on the page")
    } else if !sec.presets && (set.classic.is_some() || set.kept) {
        error("`classic` and `kept` need `#[section(presets)]`")
    } else if sec.presets && set.classic.is_some() && set.kept {
        error("`classic` or `kept`, not both")
    } else if sec.presets && set.classic.is_none() && !set.kept && !set.experimental() {
        error("expected `classic = …` or `kept`: what Classic sets it to")
    } else {
        Ok(Checked { field, set })
    }
}

fn expand_section(input: &DeriveInput) -> syn::Result<Tokens> {
    let ident = &input.ident;
    let sec = Sec::of(&input.attrs)?;
    let fields = named(input)?
        .into_iter()
        .map(|f| checked(f, &sec))
        .collect::<syn::Result<Vec<_>>>()?;

    if fields
        .iter()
        .filter(|c| c.set.gate.as_ref().is_some_and(|g| g == "Switch"))
        .count()
        > 1
    {
        return Err(syn::Error::new_spanned(
            input,
            "one `gate = Switch` at most",
        ));
    }

    let root = quote!(crate::files::setting);
    let metas = table(ident, "SETTINGS");
    let count = fields.len();

    let entries = fields.iter().map(|c| meta(c, &sec));
    let idents: Vec<&Option<Ident>> = fields.iter().map(|c| &c.field.ident).collect();
    let at = 0..count;
    let at_mut = 0..count;
    let defaults = fields.iter().map(|c| {
        let field = &c.field.ident;

        c.set.default.as_ref().map_or_else(
            || quote!(#field: Default::default()),
            |d| quote!(#field: #d),
        )
    });

    let presets = sec.presets.then(|| presets(ident, &fields));

    Ok(quote! {
        #[allow(
            clippy::cast_lossless,
            clippy::cast_precision_loss,
            reason = "a setting's range, as the page steps it"
        )]
        static #metas: [#root::Meta; #count] = [#(#entries),*];

        impl #root::Section for #ident {
            fn visit<V: #root::Visit>(&self, part: &'static #root::Part, v: &mut V) {
                #(v.field(part, &#metas[#at], &self.#idents);)*
            }

            fn visit_mut<V: #root::VisitMut>(&mut self, part: &'static #root::Part, v: &mut V) {
                #(v.field(part, &#metas[#at_mut], &mut self.#idents);)*
            }
        }

        impl Default for #ident {
            fn default() -> #ident {
                #ident { #(#defaults),* }
            }
        }

        #presets
    })
}

/// A field's `Meta`.
fn meta(c: &Checked, sec: &Sec) -> Tokens {
    let root = quote!(crate::files::setting);
    let key = key(c.field);
    let ty = &c.field.ty;
    let name = c
        .set
        .name
        .as_ref()
        .map_or_else(|| words(&key), LitStr::value);
    let about = &c.set.about;
    let group = match (&c.set.group, &sec.group) {
        _ if c.set.gated() || c.set.hidden() => quote!(None),
        (Some(g), _) | (None, Some(g)) => quote!(Some(#root::Group::#g)),
        (None, None) => quote!(None),
    };
    let gate = c
        .set
        .gate
        .clone()
        .unwrap_or_else(|| Ident::new("Always", Span::call_site()));
    let show = c
        .set
        .show
        .clone()
        .unwrap_or_else(|| Ident::new("Page", Span::call_site()));
    let range = c.set.range.as_ref().map_or_else(
        || quote!(None),
        |r| {
            quote!(Some({
                const R: std::ops::RangeInclusive<#ty> = #r;
                (*R.start() as f64)..=(*R.end() as f64)
            }))
        },
    );

    quote! {
        #root::Meta {
            key: #key,
            name: #name,
            about: #about,
            group: #group,
            gate: #root::Gate::#gate,
            show: #root::Show::#show,
            range: #range,
        }
    }
}

/// A preset section's `classic()` and `keeping()`.
fn presets(ident: &Ident, fields: &[Checked]) -> Tokens {
    let classic = fields.iter().filter_map(|c| {
        let field = &c.field.ident;

        c.set.classic.as_ref().map(|v| quote!(#field: #v))
    });
    let kept = fields
        .iter()
        .filter(|c| c.set.kept || c.set.experimental())
        .map(|c| {
            let field = &c.field.ident;

            quote!(#field: from.#field.clone())
        });

    quote! {
        impl #ident {
            /// `Preset::Classic`: each key's `classic`, else its default.
            #[must_use]
            pub fn classic() -> #ident {
                #ident { #(#classic,)* ..#ident::default() }
            }

            /// `self` with the keys no preset sets taken from `from`.
            #[must_use]
            pub fn keeping(self, from: &#ident) -> #ident {
                #ident { #(#kept,)* ..self }
            }
        }
    }
}

fn expand_sections(input: &DeriveInput) -> syn::Result<Tokens> {
    if let Some(attr) = input.attrs.iter().find(|a| a.path().is_ident(SECTION)) {
        return Err(syn::Error::new_spanned(
            attr,
            "`#[section(tab = …)]` goes on a section's field",
        ));
    }

    let ident = &input.ident;
    let fields = named(input)?;
    let root = quote!(crate::files::setting);
    let parts = table(ident, "PARTS");
    let count = fields.len();

    let entries = fields
        .iter()
        .map(|field| {
            let mut tab = Ident::new("Settings", Span::call_site());

            parsed(&field.attrs, SECTION, |meta| {
                if meta.path.is_ident("tab") {
                    tab = meta.value()?.parse()?;
                    Ok(())
                } else {
                    Err(meta.error("expected `tab`"))
                }
            })?;

            let name = key(field);

            Ok(quote!(#root::Part { name: #name, tab: #root::Tab::#tab }))
        })
        .collect::<syn::Result<Vec<_>>>()?;

    let idents: Vec<&Option<Ident>> = fields.iter().map(|f| &f.ident).collect();
    let at = 0..count;
    let at_mut = 0..count;

    Ok(quote! {
        static #parts: [#root::Part; #count] = [#(#entries),*];

        impl #ident {
            /// Each setting handed to `v`, section by section.
            pub fn visit<V: #root::Visit>(&self, v: &mut V) {
                #(#root::Section::visit(&self.#idents, &#parts[#at], v);)*
            }

            /// Each setting handed to `v` to change, section by section.
            pub fn visit_mut<V: #root::VisitMut>(&mut self, v: &mut V) {
                #(#root::Section::visit_mut(&mut self.#idents, &#parts[#at_mut], v);)*
            }

            /// The sections, in the file's order.
            #[must_use]
            pub fn parts() -> &'static [#root::Part] {
                &#parts
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn section(input: &DeriveInput) -> syn::Result<String> {
        expand_section(input).map(|t| t.to_string())
    }

    #[test]
    fn a_key_is_named_in_words() {
        assert_eq!(words("scroll_speed"), "Scroll speed");
        assert_eq!(words("bars"), "Bars");
    }

    #[test]
    fn a_field_takes_its_name_about_group_and_default() {
        let out = section(&syn::parse_quote! {
            #[section(group = Interface)]
            struct Ui {
                #[setting(name = "WASD scroll", about = "Scrolls.")]
                wasd_scroll: bool,
                #[setting(about = "", group = Display, default = 32, range = 32..=64)]
                scroll_speed: i32,
            }
        })
        .expect("expands");

        assert!(
            out.contains(r#"name : "WASD scroll" , about : "Scrolls.""#),
            "{out}"
        );
        assert!(out.contains("Group :: Interface"), "{out}");
        assert!(
            out.contains(r#"name : "Scroll speed" , about : """#),
            "{out}"
        );
        assert!(out.contains("Group :: Display"), "{out}");
        assert!(out.contains("scroll_speed : 32"), "{out}");
        assert!(out.contains("wasd_scroll : Default :: default ()"), "{out}");
        assert!(out.contains("RangeInclusive < i32 >"), "{out}");
    }

    #[test]
    fn a_preset_section_gets_classic_and_keeping() {
        let out = section(&syn::parse_quote! {
            #[section(group = Game, presets)]
            struct E {
                #[setting(about = "", classic = false)]
                speed_keys: bool,
                #[setting(about = "", show = Hidden, kept)]
                debug_keys: bool,
                #[setting(about = "", gate = Experimental)]
                wind: bool,
            }
        })
        .expect("expands");

        assert!(
            out.contains("speed_keys : false , .. E :: default ()"),
            "{out}"
        );
        assert!(
            out.contains("debug_keys : from . debug_keys . clone ()"),
            "{out}"
        );
        assert!(out.contains("wind : from . wind . clone ()"), "{out}");
    }

    #[test]
    fn a_field_without_about_is_refused() {
        assert!(
            section(&syn::parse_quote! {
                #[section(group = Game)]
                struct Ui { #[setting(name = "Bars")] bars: bool }
            })
            .is_err()
        );
    }

    #[test]
    fn a_field_without_a_group_is_refused() {
        assert!(
            section(&syn::parse_quote! {
                struct Ui { #[setting(about = "")] bars: bool }
            })
            .is_err()
        );
    }

    #[test]
    fn a_preset_key_without_classic_or_kept_is_refused() {
        assert!(
            section(&syn::parse_quote! {
                #[section(group = Game, presets)]
                struct E { #[setting(about = "")] speed_keys: bool }
            })
            .is_err()
        );
    }

    #[test]
    fn classic_without_presets_is_refused() {
        assert!(
            section(&syn::parse_quote! {
                #[section(group = Game)]
                struct E { #[setting(about = "", classic = false)] speed_keys: bool }
            })
            .is_err()
        );
    }

    #[test]
    fn an_unknown_value_is_refused() {
        assert!(
            section(&syn::parse_quote! {
                #[section(group = Game)]
                struct Ui { #[setting(label = "x", about = "")] bars: bool }
            })
            .is_err()
        );
    }

    #[test]
    fn two_switches_are_refused() {
        assert!(
            section(&syn::parse_quote! {
                #[section(group = Game)]
                struct E {
                    #[setting(about = "", gate = Switch)] a: bool,
                    #[setting(about = "", gate = Switch)] b: bool,
                }
            })
            .is_err()
        );
    }

    #[test]
    fn a_section_attribute_on_a_field_is_refused() {
        assert!(
            section(&syn::parse_quote! {
                #[section(group = Game)]
                struct Ui { #[section(tab = Enhancements)] #[setting(about = "")] bars: bool }
            })
            .is_err()
        );
    }

    #[test]
    fn a_section_attribute_on_the_root_is_refused() {
        assert!(
            expand_sections(&syn::parse_quote! {
                #[section(group = Game)]
                struct Settings { ui: Ui }
            })
            .is_err()
        );
    }

    #[test]
    fn the_sections_take_their_tabs() {
        let out = expand_sections(&syn::parse_quote! {
            struct Settings { ui: Ui, #[section(tab = Enhancements)] enhancements: E }
        })
        .expect("expands")
        .to_string();

        assert!(
            out.contains(r#"name : "ui" , tab : crate :: files :: setting :: Tab :: Settings"#),
            "{out}"
        );
        assert!(
            out.contains(
                r#"name : "enhancements" , tab : crate :: files :: setting :: Tab :: Enhancements"#
            ),
            "{out}"
        );
    }
}
