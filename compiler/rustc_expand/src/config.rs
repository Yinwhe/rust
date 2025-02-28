//! Conditional compilation stripping.

use crate::errors::{
    FeatureIncludedInEdition, FeatureNotAllowed, FeatureRemoved, FeatureRemovedReason, InvalidCfg,
    MalformedFeatureAttribute, MalformedFeatureAttributeHelp, RemoveExprNotSupported,
};
use rustc_ast::ptr::P;
use rustc_ast::token::{Delimiter, Token, TokenKind};
use rustc_ast::tokenstream::{AttrTokenStream, AttrTokenTree};
use rustc_ast::tokenstream::{DelimSpan, Spacing};
use rustc_ast::tokenstream::{LazyAttrTokenStream, TokenTree};
use rustc_ast::NodeId;
use rustc_ast::{
    self as ast, AttrKind, AttrStyle, Attribute, HasAttrs, HasTokens, MetaItem,
    MetaItemKind, NestedMetaItem,
};
use rustc_attr as attr;
use rustc_data_structures::fx::FxHashMap;
use rustc_data_structures::map_in_place::MapInPlace;
use rustc_feature::{Feature, Features, State as FeatureState};
use rustc_feature::{
    ACCEPTED_FEATURES, ACTIVE_FEATURES, REMOVED_FEATURES, STABLE_REMOVED_FEATURES,
};
use rustc_parse::validate_attr;
use rustc_session::parse::feature_err;
use rustc_session::Session;
use rustc_span::edition::{Edition, ALL_EDITIONS};
use rustc_span::symbol::{sym, Symbol};
use rustc_span::{Span, DUMMY_SP};
use ast::AttrArgs;


/// A folder that strips out items that do not belong in the current configuration.
pub struct StripUnconfigured<'a> {
    pub sess: &'a Session,
    pub features: Option<&'a Features>,
    /// If `true`, perform cfg-stripping on attached tokens.
    /// This is only used for the input to derive macros,
    /// which needs eager expansion of `cfg` and `cfg_attr`
    pub config_tokens: bool,
    pub lint_node_id: NodeId,
}

// # NOTE HERE
pub struct ConfigFeatures<'a> {
    pub sess: &'a Session,
    pub origin_cfg_attrs_datas: Vec<(Vec<MetaItem>, Attribute)>,
    pub visulized_cfg_attrs_datas: Vec<(Vec<String>, String)>,
    pub processed_cfg_attrs_datas: Vec<(Vec<String>, String)>,
}

impl<'a> ConfigFeatures<'a> {
    pub fn analysis_krate_attrs(&mut self, attrs: Vec<ast::Attribute>) {
        for attr in attrs {
            self._analysis(attr, vec![]);
        }

        self.process_analysis_result();
    }

    pub fn print_processed(&self) {
        self.processed_cfg_attrs_datas
            .iter()
            .map(|(conds, feat)| {
                println!("processed ([{}], {})", conds.join(","), feat);
            })
            .count();
    }

    pub fn print_visulized(&self) {
        self.visulized_cfg_attrs_datas
            .iter()
            .map(|(conds, attr)| {
                println!("formatori ([{}], {})", conds.join(","), attr);
            })
            .count();
    }

    pub fn process_analysis_result(&mut self) {
        let mut items = vec![];
        let mut viz_items = vec![];

        for (conds, attr) in &self.origin_cfg_attrs_datas {
            let mut pro_conds = vec![vec![]];
            let mut pro_feats = vec![];

            if !attr.has_name(sym::feature) {
                continue;
            }

            let viz_conds: Vec<String> =
                conds.iter().map(|cond| self.visulize_cfg_cond(cond)).collect();

            for cond in conds {
                let tmp = self.process_cfg_cond(cond);
                pro_conds = vec_mul(pro_conds, tmp.into_iter().map(|s| vec![s]).collect());
            }

            if let AttrKind::Normal(item) = &attr.kind {
                if let AttrArgs::Delimited(delimargs) = &item.item.args {
                    let tokens = &delimargs.tokens;
                    for token in tokens.trees() {
                        if let TokenTree::Token(token, _) = token {
                            if let Some((ident, _)) = token.ident() {
                                pro_feats.push(ident.name.to_string());
                                viz_items.push((viz_conds.clone(), ident.name.to_string()));
                            }
                        }
                    }
                } else {
                    panic!("rustc resolve feature fails");
                }
            } else {
                panic!("rustc resolve feature fails");
            }

            pro_feats
                .into_iter()
                .map(|feat| {
                    pro_conds
                        .clone()
                        .into_iter()
                        .map(|conds| items.push((conds, feat.clone())))
                        .count()
                })
                .count();
        }

        viz_items.into_iter().map(|item| self.assign_vis(item)).count();
        items.into_iter().map(|item| self.assign_pro(item)).count();
    }

    fn _analysis(&mut self, attr: Attribute, meta: Vec<MetaItem>) {
        if attr.has_name(sym::cfg_attr) {
            let Some((cfg_predicate, expanded_attrs)) =
                rustc_parse::parse_cfg_attr(&attr, &self.sess.parse_sess) else {
                    return;
                };

            expanded_attrs
                .into_iter()
                .map(|item| {
                    let cfg_predicate = cfg_predicate.clone();
                    let mut meta = meta.clone();

                    meta.push(cfg_predicate);
                    self._analysis(self.expand_cfg_attr_item(&attr, item), meta);
                })
                .count();
        } else {
            self.assign_ori((meta.clone(), attr));
        }
    }

    #[inline]
    fn assign_ori(&mut self, item: (Vec<MetaItem>, Attribute)) {
        self.origin_cfg_attrs_datas.push(item);
    }

    #[inline]
    fn assign_pro(&mut self, item: (Vec<String>, String)) {
        self.processed_cfg_attrs_datas.push(item);
    }

    #[inline]
    fn assign_vis(&mut self, item: (Vec<String>, String)) {
        self.visulized_cfg_attrs_datas.push(item);
    }

    fn visulize_cfg_cond(&self, cond: &MetaItem) -> String {
        let req = match &cond.kind {
            MetaItemKind::Word => cond.ident().expect("rustc resolve feature fails").as_str().to_string(),
            MetaItemKind::NameValue(lit) => {
                format!("{} = {}", cond.ident().expect("rustc resolve feature fails").as_str(), lit.symbol.as_str(),)
            }
            MetaItemKind::List(nmetas) => {
                let mut req = String::new();

                for nmeta in nmetas {
                    match nmeta {
                        NestedMetaItem::MetaItem(meta) => {
                            req.push_str(&self.visulize_cfg_cond(meta));
                            req.push_str(",")
                        }
                        _ => panic!("rustc resolve feature fails"),
                    }
                }
                req.pop();
                format!("{}({})", cond.ident().expect("rustc resolve feature fails").as_str(), req)
            }
        };

        req
    }

    fn process_cfg_cond(&self, cond: &MetaItem) -> Vec<String> {
        let left_val = cond.ident().expect("rustc resolve feature fails").as_str().to_string();

        // cases to be ignored
        if left_val.starts_with("target") {
            return vec![];
        }
        // if left_val == "unix" || left_val == "windows" || left_val == "nightly" || left_val == "RUSTC_WITH_SPECIALIZATION" {
        //     return vec![];
        // }

        let req = match &cond.kind {
            MetaItemKind::Word => {
                vec![left_val]
            }
            MetaItemKind::NameValue(lit) => {
                vec![format!("{} = {}", &left_val, lit.symbol.as_str())]
            }
            MetaItemKind::List(nmetas) => {
                let mut nest_conds = vec![];

                for nmeta in nmetas {
                    match nmeta {
                        NestedMetaItem::MetaItem(meta) => {
                            nest_conds.push(self.process_cfg_cond(meta));
                        }
                        _ => panic!("rustc resolve feature fails"),
                    }
                }

                if left_val.as_str() == "any" {
                    // Union
                   nest_conds.into_iter().flatten().collect()
                } else if left_val.as_str() == "all" {
                    // Multi
                    let mut mul_res = vec![];
                    for conds in nest_conds {
                        mul_res = vec_mul(mul_res, conds.into_iter().map(|s| vec![s]).collect());
                    }
                    mul_res.into_iter().map(|conds| if conds.len() == 1 {
                        conds.join(",")
                    } else {
                        format!("all({})", conds.join(","))
                    }).collect()
                } else if left_val.as_str() == "not" {
                    if nest_conds.len() != 1 {
                        panic!("rustc resolve feature fails");
                    }

                    let conds = nest_conds.first().unwrap();
                    if conds.len() == 1 {
                        vec![format!("not({})", conds.join(","))]
                    } else {
                        vec![format!("not(any({}))", conds.join(","))]
                    }

                } else {
                    panic!("rustc resolve feature fails");
                }
            }
        };

        req
    }

    fn expand_cfg_attr_item(
        &self,
        attr: &Attribute,
        (item, item_span): (ast::AttrItem, Span),
    ) -> Attribute {
        let orig_tokens = attr.tokens();

        // We are taking an attribute of the form `#[cfg_attr(pred, attr)]`
        // and producing an attribute of the form `#[attr]`. We
        // have captured tokens for `attr` itself, but we need to
        // synthesize tokens for the wrapper `#` and `[]`, which
        // we do below.

        // Use the `#` in `#[cfg_attr(pred, attr)]` as the `#` token
        // for `attr` when we expand it to `#[attr]`
        let mut orig_trees = orig_tokens.into_trees();
        let TokenTree::Token(pound_token @ Token { kind: TokenKind::Pound, .. }, _) = orig_trees.next().unwrap() else {
            panic!("Bad tokens for attribute {:?}", attr);
        };
        let pound_span = pound_token.span;

        let mut trees = vec![AttrTokenTree::Token(pound_token, Spacing::Alone)];
        if attr.style == AttrStyle::Inner {
            // For inner attributes, we do the same thing for the `!` in `#![some_attr]`
            let TokenTree::Token(bang_token @ Token { kind: TokenKind::Not, .. }, _) = orig_trees.next().unwrap() else {
                panic!("Bad tokens for attribute {:?}", attr);
            };
            trees.push(AttrTokenTree::Token(bang_token, Spacing::Alone));
        }
        // We don't really have a good span to use for the synthesized `[]`
        // in `#[attr]`, so just use the span of the `#` token.
        let bracket_group = AttrTokenTree::Delimited(
            DelimSpan::from_single(pound_span),
            Delimiter::Bracket,
            item.tokens
                .as_ref()
                .unwrap_or_else(|| panic!("Missing tokens for {:?}", item))
                .to_attr_token_stream(),
        );
        trees.push(bracket_group);
        let tokens = Some(LazyAttrTokenStream::new(AttrTokenStream::new(trees)));
        let attr = attr::mk_attr_from_item(
            &self.sess.parse_sess.attr_id_generator,
            item,
            tokens,
            attr.style,
            item_span,
        );
        if attr.has_name(sym::crate_type) {
            self.sess.parse_sess.buffer_lint(
                rustc_lint_defs::builtin::DEPRECATED_CFG_ATTR_CRATE_TYPE_NAME,
                attr.span,
                ast::CRATE_NODE_ID,
                "`crate_type` within an `#![cfg_attr] attribute is deprecated`",
            );
        }
        if attr.has_name(sym::crate_name) {
            self.sess.parse_sess.buffer_lint(
                rustc_lint_defs::builtin::DEPRECATED_CFG_ATTR_CRATE_TYPE_NAME,
                attr.span,
                ast::CRATE_NODE_ID,
                "`crate_name` within an `#![cfg_attr] attribute is deprecated`",
            );
        }
        attr
    }
}

fn vec_mul<T: Clone>(a: Vec<Vec<T>>, b: Vec<Vec<T>>) -> Vec<Vec<T>> {
    if a.is_empty() {
        return b;
    } else if b.is_empty() {
        return a;
    }

    a.into_iter()
        .map(|item1| {
            b.clone()
                .into_iter()
                .map(|item2| {
                    let mut t = item1.clone();
                    t.extend(item2);
                    t
                })
                .collect::<Vec<Vec<T>>>()
        })
        .flatten()
        .collect()
}

fn get_features(sess: &Session, krate_attrs: &[ast::Attribute]) -> Features {
    fn feature_removed(sess: &Session, span: Span, reason: Option<&str>) {
        sess.emit_err(FeatureRemoved {
            span,
            reason: reason.map(|reason| FeatureRemovedReason { reason }),
        });
    }

    fn active_features_up_to(edition: Edition) -> impl Iterator<Item = &'static Feature> {
        ACTIVE_FEATURES.iter().filter(move |feature| {
            if let Some(feature_edition) = feature.edition {
                feature_edition <= edition
            } else {
                false
            }
        })
    }

    let mut features = Features::default();
    let mut edition_enabled_features = FxHashMap::default();
    let crate_edition = sess.edition();

    for &edition in ALL_EDITIONS {
        if edition <= crate_edition {
            // The `crate_edition` implies its respective umbrella feature-gate
            // (i.e., `#![feature(rust_20XX_preview)]` isn't needed on edition 20XX).
            edition_enabled_features.insert(edition.feature_name(), edition);
        }
    }

    for feature in active_features_up_to(crate_edition) {
        feature.set(&mut features, DUMMY_SP);
        edition_enabled_features.insert(feature.name, crate_edition);
    }

    // Process the edition umbrella feature-gates first, to ensure
    // `edition_enabled_features` is completed before it's queried.
    for attr in krate_attrs {
        if !attr.has_name(sym::feature) {
            continue;
        }

        let Some(list) = attr.meta_item_list() else {
            continue;
        };

        for mi in list {
            if !mi.is_word() {
                continue;
            }

            let name = mi.name_or_empty();

            let edition = ALL_EDITIONS.iter().find(|e| name == e.feature_name()).copied();
            if let Some(edition) = edition {
                if edition <= crate_edition {
                    continue;
                }

                for feature in active_features_up_to(edition) {
                    // FIXME(Manishearth) there is currently no way to set
                    // lib features by edition
                    feature.set(&mut features, DUMMY_SP);
                    edition_enabled_features.insert(feature.name, edition);
                }
            }
        }
    }

    for attr in krate_attrs {
        if !attr.has_name(sym::feature) {
            continue;
        }

        let Some(list) = attr.meta_item_list() else {
            continue;
        };

        for mi in list {
            let name = match mi.ident() {
                Some(ident) if mi.is_word() => ident.name,
                Some(ident) => {
                    sess.emit_err(MalformedFeatureAttribute {
                        span: mi.span(),
                        help: MalformedFeatureAttributeHelp::Suggestion {
                            span: mi.span(),
                            suggestion: ident.name,
                        },
                    });
                    continue;
                }
                None => {
                    sess.emit_err(MalformedFeatureAttribute {
                        span: mi.span(),
                        help: MalformedFeatureAttributeHelp::Label { span: mi.span() },
                    });
                    continue;
                }
            };

            if let Some(&edition) = edition_enabled_features.get(&name) {
                sess.emit_warning(FeatureIncludedInEdition {
                    span: mi.span(),
                    feature: name,
                    edition,
                });
                continue;
            }

            if ALL_EDITIONS.iter().any(|e| name == e.feature_name()) {
                // Handled in the separate loop above.
                continue;
            }

            let removed = REMOVED_FEATURES.iter().find(|f| name == f.name);
            let stable_removed = STABLE_REMOVED_FEATURES.iter().find(|f| name == f.name);
            if let Some(Feature { state, .. }) = removed.or(stable_removed) {
                if let FeatureState::Removed { reason } | FeatureState::Stabilized { reason } =
                    state
                {
                    feature_removed(sess, mi.span(), *reason);
                    continue;
                }
            }

            if let Some(Feature { since, .. }) = ACCEPTED_FEATURES.iter().find(|f| name == f.name) {
                let since = Some(Symbol::intern(since));
                features.declared_lang_features.push((name, mi.span(), since));
                features.active_features.insert(name);
                continue;
            }

            if let Some(allowed) = sess.opts.unstable_opts.allow_features.as_ref() {
                if allowed.iter().all(|f| name.as_str() != f) {
                    sess.emit_err(FeatureNotAllowed { span: mi.span(), name });
                    continue;
                }
            }

            if let Some(f) = ACTIVE_FEATURES.iter().find(|f| name == f.name) {
                f.set(&mut features, mi.span());
                features.declared_lang_features.push((name, mi.span(), None));
                features.active_features.insert(name);
                continue;
            }

            features.declared_lib_features.push((name, mi.span()));
            features.active_features.insert(name);
        }
    }

    features
}

/// `cfg_attr`-process the crate's attributes and compute the crate's features.
pub fn features(
    sess: &Session,
    mut krate: ast::Crate,
    lint_node_id: NodeId,
) -> (ast::Crate, Features) {
    let mut strip_unconfigured =
        StripUnconfigured { sess, features: None, config_tokens: false, lint_node_id };

    let unconfigured_attrs = krate.attrs.clone();
    let diag = &sess.parse_sess.span_diagnostic;
    let err_count = diag.err_count();
    let features = match strip_unconfigured.configure_krate_attrs(krate.attrs) {
        None => {
            // The entire crate is unconfigured.
            krate.attrs = ast::AttrVec::new();
            krate.items = Vec::new();
            Features::default()
        }
        Some(attrs) => {
            krate.attrs = attrs;
            let features = get_features(sess, &krate.attrs);
            if err_count == diag.err_count() {
                // Avoid reconfiguring malformed `cfg_attr`s.
                strip_unconfigured.features = Some(&features);
                // Run configuration again, this time with features available
                // so that we can perform feature-gating.
                strip_unconfigured.configure_krate_attrs(unconfigured_attrs);
            }
            features
        }
    };
    (krate, features)
}

#[macro_export]
macro_rules! configure {
    ($this:ident, $node:ident) => {
        match $this.configure($node) {
            Some(node) => node,
            None => return Default::default(),
        }
    };
}

impl<'a> StripUnconfigured<'a> {
    pub fn configure<T: HasAttrs + HasTokens>(&self, mut node: T) -> Option<T> {
        self.process_cfg_attrs(&mut node);
        if self.in_cfg(node.attrs()) {
            self.try_configure_tokens(&mut node);
            Some(node)
        } else {
            None
        }
    }

    fn try_configure_tokens<T: HasTokens>(&self, node: &mut T) {
        if self.config_tokens {
            if let Some(Some(tokens)) = node.tokens_mut() {
                let attr_stream = tokens.to_attr_token_stream();
                *tokens = LazyAttrTokenStream::new(self.configure_tokens(&attr_stream));
            }
        }
    }

    fn configure_krate_attrs(&self, mut attrs: ast::AttrVec) -> Option<ast::AttrVec> {
        attrs.flat_map_in_place(|attr| self.process_cfg_attr(attr));
        if self.in_cfg(&attrs) {
            Some(attrs)
        } else {
            None
        }
    }

    /// Performs cfg-expansion on `stream`, producing a new `AttrTokenStream`.
    /// This is only used during the invocation of `derive` proc-macros,
    /// which require that we cfg-expand their entire input.
    /// Normal cfg-expansion operates on parsed AST nodes via the `configure` method
    fn configure_tokens(&self, stream: &AttrTokenStream) -> AttrTokenStream {
        fn can_skip(stream: &AttrTokenStream) -> bool {
            stream.0.iter().all(|tree| match tree {
                AttrTokenTree::Attributes(_) => false,
                AttrTokenTree::Token(..) => true,
                AttrTokenTree::Delimited(_, _, inner) => can_skip(inner),
            })
        }

        if can_skip(stream) {
            return stream.clone();
        }

        let trees: Vec<_> = stream
            .0
            .iter()
            .flat_map(|tree| match tree.clone() {
                AttrTokenTree::Attributes(mut data) => {
                    data.attrs.flat_map_in_place(|attr| self.process_cfg_attr(attr));

                    if self.in_cfg(&data.attrs) {
                        data.tokens = LazyAttrTokenStream::new(
                            self.configure_tokens(&data.tokens.to_attr_token_stream()),
                        );
                        Some(AttrTokenTree::Attributes(data)).into_iter()
                    } else {
                        None.into_iter()
                    }
                }
                AttrTokenTree::Delimited(sp, delim, mut inner) => {
                    inner = self.configure_tokens(&inner);
                    Some(AttrTokenTree::Delimited(sp, delim, inner))
                        .into_iter()
                }
                AttrTokenTree::Token(ref token, _) if let TokenKind::Interpolated(nt) = &token.kind => {
                    panic!(
                        "Nonterminal should have been flattened at {:?}: {:?}",
                        token.span, nt
                    );
                }
                AttrTokenTree::Token(token, spacing) => {
                    Some(AttrTokenTree::Token(token, spacing)).into_iter()
                }
            })
            .collect();
        AttrTokenStream::new(trees)
    }

    /// Parse and expand all `cfg_attr` attributes into a list of attributes
    /// that are within each `cfg_attr` that has a true configuration predicate.
    ///
    /// Gives compiler warnings if any `cfg_attr` does not contain any
    /// attributes and is in the original source code. Gives compiler errors if
    /// the syntax of any `cfg_attr` is incorrect.
    fn process_cfg_attrs<T: HasAttrs>(&self, node: &mut T) {
        node.visit_attrs(|attrs| {
            attrs.flat_map_in_place(|attr| self.process_cfg_attr(attr));
        });
    }

    fn process_cfg_attr(&self, attr: Attribute) -> Vec<Attribute> {
        if attr.has_name(sym::cfg_attr) {
            self.expand_cfg_attr(attr, true)
        } else {
            vec![attr]
        }
    }

    /// Parse and expand a single `cfg_attr` attribute into a list of attributes
    /// when the configuration predicate is true, or otherwise expand into an
    /// empty list of attributes.
    ///
    /// Gives a compiler warning when the `cfg_attr` contains no attributes and
    /// is in the original source file. Gives a compiler error if the syntax of
    /// the attribute is incorrect.
    pub(crate) fn expand_cfg_attr(&self, attr: Attribute, recursive: bool) -> Vec<Attribute> {
        let Some((cfg_predicate, expanded_attrs)) =
            rustc_parse::parse_cfg_attr(&attr, &self.sess.parse_sess) else {
                return vec![];
            };

        // Lint on zero attributes in source.
        if expanded_attrs.is_empty() {
            self.sess.parse_sess.buffer_lint(
                rustc_lint_defs::builtin::UNUSED_ATTRIBUTES,
                attr.span,
                ast::CRATE_NODE_ID,
                "`#[cfg_attr]` does not expand to any attributes",
            );
        }

        if !attr::cfg_matches(
            &cfg_predicate,
            &self.sess.parse_sess,
            self.lint_node_id,
            self.features,
        ) {
            return vec![];
        }

        if recursive {
            // We call `process_cfg_attr` recursively in case there's a
            // `cfg_attr` inside of another `cfg_attr`. E.g.
            //  `#[cfg_attr(false, cfg_attr(true, some_attr))]`.
            expanded_attrs
                .into_iter()
                .flat_map(|item| self.process_cfg_attr(self.expand_cfg_attr_item(&attr, item)))
                .collect()
        } else {
            expanded_attrs.into_iter().map(|item| self.expand_cfg_attr_item(&attr, item)).collect()
        }
    }

    fn expand_cfg_attr_item(
        &self,
        attr: &Attribute,
        (item, item_span): (ast::AttrItem, Span),
    ) -> Attribute {
        let orig_tokens = attr.tokens();

        // We are taking an attribute of the form `#[cfg_attr(pred, attr)]`
        // and producing an attribute of the form `#[attr]`. We
        // have captured tokens for `attr` itself, but we need to
        // synthesize tokens for the wrapper `#` and `[]`, which
        // we do below.

        // Use the `#` in `#[cfg_attr(pred, attr)]` as the `#` token
        // for `attr` when we expand it to `#[attr]`
        let mut orig_trees = orig_tokens.into_trees();
        let TokenTree::Token(pound_token @ Token { kind: TokenKind::Pound, .. }, _) = orig_trees.next().unwrap() else {
            panic!("Bad tokens for attribute {:?}", attr);
        };
        let pound_span = pound_token.span;

        let mut trees = vec![AttrTokenTree::Token(pound_token, Spacing::Alone)];
        if attr.style == AttrStyle::Inner {
            // For inner attributes, we do the same thing for the `!` in `#![some_attr]`
            let TokenTree::Token(bang_token @ Token { kind: TokenKind::Not, .. }, _) = orig_trees.next().unwrap() else {
                panic!("Bad tokens for attribute {:?}", attr);
            };
            trees.push(AttrTokenTree::Token(bang_token, Spacing::Alone));
        }
        // We don't really have a good span to use for the synthesized `[]`
        // in `#[attr]`, so just use the span of the `#` token.
        let bracket_group = AttrTokenTree::Delimited(
            DelimSpan::from_single(pound_span),
            Delimiter::Bracket,
            item.tokens
                .as_ref()
                .unwrap_or_else(|| panic!("Missing tokens for {:?}", item))
                .to_attr_token_stream(),
        );
        trees.push(bracket_group);
        let tokens = Some(LazyAttrTokenStream::new(AttrTokenStream::new(trees)));
        let attr = attr::mk_attr_from_item(
            &self.sess.parse_sess.attr_id_generator,
            item,
            tokens,
            attr.style,
            item_span,
        );
        if attr.has_name(sym::crate_type) {
            self.sess.parse_sess.buffer_lint(
                rustc_lint_defs::builtin::DEPRECATED_CFG_ATTR_CRATE_TYPE_NAME,
                attr.span,
                ast::CRATE_NODE_ID,
                "`crate_type` within an `#![cfg_attr] attribute is deprecated`",
            );
        }
        if attr.has_name(sym::crate_name) {
            self.sess.parse_sess.buffer_lint(
                rustc_lint_defs::builtin::DEPRECATED_CFG_ATTR_CRATE_TYPE_NAME,
                attr.span,
                ast::CRATE_NODE_ID,
                "`crate_name` within an `#![cfg_attr] attribute is deprecated`",
            );
        }
        attr
    }

    /// Determines if a node with the given attributes should be included in this configuration.
    fn in_cfg(&self, attrs: &[Attribute]) -> bool {
        attrs.iter().all(|attr| !is_cfg(attr) || self.cfg_true(attr))
    }

    pub(crate) fn cfg_true(&self, attr: &Attribute) -> bool {
        let meta_item = match validate_attr::parse_meta(&self.sess.parse_sess, attr) {
            Ok(meta_item) => meta_item,
            Err(mut err) => {
                err.emit();
                return true;
            }
        };
        parse_cfg(&meta_item, &self.sess).map_or(true, |meta_item| {
            attr::cfg_matches(&meta_item, &self.sess.parse_sess, self.lint_node_id, self.features)
        })
    }

    /// If attributes are not allowed on expressions, emit an error for `attr`
    #[instrument(level = "trace", skip(self))]
    pub(crate) fn maybe_emit_expr_attr_err(&self, attr: &Attribute) {
        if !self.features.map_or(true, |features| features.stmt_expr_attributes) {
            let mut err = feature_err(
                &self.sess.parse_sess,
                sym::stmt_expr_attributes,
                attr.span,
                "attributes on expressions are experimental",
            );

            if attr.is_doc_comment() {
                err.help("`///` is for documentation comments. For a plain comment, use `//`.");
            }

            err.emit();
        }
    }

    #[instrument(level = "trace", skip(self))]
    pub fn configure_expr(&self, expr: &mut P<ast::Expr>, method_receiver: bool) {
        if !method_receiver {
            for attr in expr.attrs.iter() {
                self.maybe_emit_expr_attr_err(attr);
            }
        }

        // If an expr is valid to cfg away it will have been removed by the
        // outer stmt or expression folder before descending in here.
        // Anything else is always required, and thus has to error out
        // in case of a cfg attr.
        //
        // N.B., this is intentionally not part of the visit_expr() function
        //     in order for filter_map_expr() to be able to avoid this check
        if let Some(attr) = expr.attrs().iter().find(|a| is_cfg(*a)) {
            self.sess.emit_err(RemoveExprNotSupported { span: attr.span });
        }

        self.process_cfg_attrs(expr);
        self.try_configure_tokens(&mut *expr);
    }
}

pub fn parse_cfg<'a>(meta_item: &'a MetaItem, sess: &Session) -> Option<&'a MetaItem> {
    let span = meta_item.span;
    match meta_item.meta_item_list() {
        None => {
            sess.emit_err(InvalidCfg::NotFollowedByParens { span });
            None
        }
        Some([]) => {
            sess.emit_err(InvalidCfg::NoPredicate { span });
            None
        }
        Some([_, .., l]) => {
            sess.emit_err(InvalidCfg::MultiplePredicates { span: l.span() });
            None
        }
        Some([single]) => match single.meta_item() {
            Some(meta_item) => Some(meta_item),
            None => {
                sess.emit_err(InvalidCfg::PredicateLiteral { span: single.span() });
                None
            }
        },
    }
}

fn is_cfg(attr: &Attribute) -> bool {
    attr.has_name(sym::cfg)
}
