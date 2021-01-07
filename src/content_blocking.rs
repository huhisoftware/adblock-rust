//! Transforms filter rules into content blocking syntax used on iOS and MacOS.

use crate::filters::network::{NetworkFilter, NetworkFilterMask};
use crate::filters::cosmetic::CosmeticFilter;
use crate::lists::ParsedFilter;

use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};

use std::convert::{TryFrom, TryInto};

/// Rust representation of a single content blocking rule.
///
/// This can be deserialized with `serde_json` directly into the correct format.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct CbRule {
    pub action: CbAction,
    pub trigger: CbTrigger,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct CbAction {
    #[serde(rename = "type")]
    pub typ: CbType,
    /// Specify a string that defines a selector list. This value is required when the action type
    /// is css-display-none. If it's not, the selector field is ignored by Safari. Use CSS
    /// identifiers as the individual selector values, separated by commas. Safari and WebKit
    /// supports all of its CSS selectors for Safari content-blocking rules.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selector: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CbType {
    /// Stops loading of the resource. If the resource was cached, the cache is ignored.
    Block,
    /// Strips cookies from the header before sending to the server. Only cookies otherwise
    /// acceptable to Safari's privacy policy can be blocked. Combining with ignore-previous-rules
    /// doesn't override the browser’s privacy settings.
    BlockCookies,
    /// Hides elements of the page based on a CSS selector. A selector field contains the selector
    /// list. Any matching element has its display property set to none, which hides it.
    CssDisplayNone,
    /// Ignores previously triggered actions.
    IgnorePreviousRules,
    /// Changes a URL from http to https. URLs with a specified (nondefault) port and links using
    /// other protocols are unaffected.
    MakeHttps,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CbLoadType {
    FirstParty,
    ThirdParty,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CbResourceType {
    Document,
    Image,
    StyleSheet,
    Script,
    Font,
    Raw,
    SvgDocument,
    Media,
    Popup,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct CbTrigger {
    /// Specifies a pattern to match the URL against.
    pub url_filter: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// A Boolean value. The default value is false.
    pub url_filter_is_case_sensitive: Option<bool>,
    /// An array of strings matched to a URL's domain; limits action to a list of specific domains.
    /// Values must be lowercase ASCII, or punycode for non-ASCII. Add * in front to match domain
    /// and subdomains. Can't be used with unless-domain.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub if_domain: Option<Vec<String>>,
    /// An array of strings matched to a URL's domain; acts on any site except domains in a
    /// provided list. Values must be lowercase ASCII, or punycode for non-ASCII. Add * in front to
    /// match domain and subdomains. Can't be used with if-domain.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unless_domain: Option<Vec<String>>,
    /// An array of strings representing the resource types (how the browser intends to use the
    /// resource) that the rule should match. If not specified, the rule matches all resource
    /// types. Valid values: document, image, style-sheet, script, font, raw (Any untyped load),
    /// svg-document, media, popup.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_type: Option<std::collections::HashSet<CbResourceType>>,
    /// An array of strings that can include one of two mutually exclusive values. If not
    /// specified, the rule matches all load types. first-party is triggered only if the resource
    /// has the same scheme, domain, and port as the main page resource. third-party is triggered
    /// if the resource is not from the same domain as the main page resource.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub load_type: Vec<CbLoadType>,
    /// An array of strings matched to the entire main document URL; limits the action to a
    /// specific list of URL patterns. Values must be lowercase ASCII, or punycode for non-ASCII.
    /// Can't be used with unless-top-url.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub if_top_url: Option<Vec<String>>,
    /// An array of strings matched to the entire main document URL; acts on any site except URL
    /// patterns in provided list. Values must be lowercase ASCII, or punycode for non-ASCII. Can't
    /// be used with if-top-url.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unless_top_url: Option<Vec<String>>,
}

#[derive(Debug)]
pub enum CbRuleCreationFailure {
    /// Currently, only filter rules parsed in debug mode can be translated into equivalent content
    /// blocking syntax.
    NeedsDebugMode,
    /// Content blocking rules cannot have if-domain and unless-domain together at the same time.
    UnlessAndIfDomainTogetherUnsupported,
    /// A network filter rule with only the given content type flags was provided, and none of them
    /// are supported. If at least one supported content type is provided, no failure will occur
    /// and unsupported types will be silently dropped.
    NoSupportedNetworkOptions(NetworkFilterMask),
    /// Network rules with redirect options cannot be represented in content blocking syntax.
    NetworkRedirectUnsupported,
    /// Network rules with fuzzy matching options cannot be represented in content blocking syntax.
    NetworkFuzzyMatchUnsupported,
    /// Network rules with generichide options cannot be supported in content blocking syntax.
    NetworkGenerichideUnsupported,
    /// Network rules with explicitcancel options cannot be supported in content blocking syntax.
    NetworkExplicitCancelUnsupported,
    /// Network rules with badfilter options cannot be supported in content blocking syntax.
    NetworkBadFilterUnsupported,
    /// Network rules with csp options cannot be supported in content blocking syntax.
    NetworkCspUnsupported,
    /// `Blocker`-internal `NetworkFilter`s can be represented in optimized form, but these cannot
    /// be currently converted into content blocking syntax.
    OptimizedRulesUnsupported,
    /// Cosmetic rules with entities (e.g. google.*) rather than hostnames cannot be represented in
    /// content blocking syntax.
    CosmeticEntitiesUnsupported,
    /// Cosmetic rules with custom style specification (i.e. `:style(...)`) cannot be represented
    /// in content blocking syntax.
    CosmeticStyleRulesNotSupported,
    /// Cosmetic rules with scriptlet injections (i.e. `+js(...)`) cannot be represented in content
    /// blocking syntax.
    ScriptletInjectionsNotSupported,
}

impl TryFrom<ParsedFilter> for CbRuleEquivalent {
    type Error = CbRuleCreationFailure;

    fn try_from(v: ParsedFilter) -> Result<Self, Self::Error> {
        match v {
            ParsedFilter::Network(f) => f.try_into(),
            ParsedFilter::Cosmetic(f) => Ok(Self::SingleRule(f.try_into()?)),
        }
    }
}

fn non_empty(v: Vec<String>) -> Option<Vec<String>> {
    if v.len() > 0 { Some(v) } else { None }
}

/// Some adblock rules cannot be directly represented by a single content blocking rule. This enum
/// serves as an intermediate conversion step that provides extra context on why one rule turned
/// into multiple rules.
///
/// The contained rules can be accessed using `IntoIterator`.
pub enum CbRuleEquivalent {
    /// In most successful cases, an ABP rule can be converted into a single content blocking rule.
    SingleRule(CbRule),
    /// If a network rule has more than one specified resource type, one of those types is
    /// `Document`, and no load type is specified, then the rule should be split into two content
    /// blocking rules: the first has all original resource types except `Document`, and the second
    /// only specifies `Document` with a third-party load type.
    SplitDocument(CbRule, CbRule),
}

impl IntoIterator for CbRuleEquivalent {
    type Item = CbRule;
    type IntoIter = CbRuleEquivalentIterator;

    fn into_iter(self) -> Self::IntoIter {
        match self {
            Self::SingleRule(r) => CbRuleEquivalentIterator { rules: [Some(r), None], index: 0 },
            Self::SplitDocument(r1, r2) => CbRuleEquivalentIterator { rules: [Some(r1), Some(r2)], index: 0 },
        }
    }
}

pub struct CbRuleEquivalentIterator {
    rules: [Option<CbRule>; 2],
    index: usize,
}

impl Iterator for CbRuleEquivalentIterator {
    type Item = CbRule;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.rules.len() {
            return None;
        }
        let result = self.rules[self.index].take();
        self.index += 1;
        result
    }
}

impl TryFrom<NetworkFilter> for CbRuleEquivalent {
    type Error = CbRuleCreationFailure;

    fn try_from(v: NetworkFilter) -> Result<Self, Self::Error> {
        static SPECIAL_CHARS: Lazy<Regex> = Lazy::new(|| Regex::new(r##"([.+?^${}()|\[\]])"##).unwrap());
        static REPLACE_WILDCARDS: Lazy<Regex> = Lazy::new(|| Regex::new(r##"\*"##).unwrap());
        static TRAILING_SEPARATOR: Lazy<Regex> = Lazy::new(|| Regex::new(r##"\^$"##).unwrap());
        if let Some(raw_line) = v.raw_line {
            if v.redirect.is_some() {
                return Err(CbRuleCreationFailure::NetworkRedirectUnsupported);
            }
            if v.mask.contains(NetworkFilterMask::FUZZY_MATCH) {
                return Err(CbRuleCreationFailure::NetworkFuzzyMatchUnsupported);
            }
            if v.mask.contains(NetworkFilterMask::GENERIC_HIDE) {
                return Err(CbRuleCreationFailure::NetworkGenerichideUnsupported);
            }
            if v.mask.contains(NetworkFilterMask::EXPLICIT_CANCEL) {
                return Err(CbRuleCreationFailure::NetworkExplicitCancelUnsupported);
            }
            if v.mask.contains(NetworkFilterMask::BAD_FILTER) {
                return Err(CbRuleCreationFailure::NetworkBadFilterUnsupported);
            }
            if v.mask.contains(NetworkFilterMask::IS_CSP) {
                return Err(CbRuleCreationFailure::NetworkCspUnsupported);
            }

            let load_type = if v.mask.contains(NetworkFilterMask::THIRD_PARTY | NetworkFilterMask::FIRST_PARTY) {
                vec![]
            } else if v.mask.contains(NetworkFilterMask::THIRD_PARTY) {
                vec![CbLoadType::ThirdParty]
            } else if v.mask.contains(NetworkFilterMask::FIRST_PARTY) {
                vec![CbLoadType::FirstParty]
            } else {
                vec![]
            };

            let url_filter = match (v.filter, v.hostname) {
                (crate::filters::network::FilterPart::AnyOf(_), _) => return Err(CbRuleCreationFailure::OptimizedRulesUnsupported),
                (crate::filters::network::FilterPart::Simple(part), Some(hostname)) => {
                    let without_trailing_separator = TRAILING_SEPARATOR.replace_all(&part, "");
                    let escaped_special_chars = SPECIAL_CHARS.replace_all(&without_trailing_separator, r##"\$1"##);
                    let with_fixed_wildcards = REPLACE_WILDCARDS.replace_all(&escaped_special_chars, ".*");

                    let mut url_filter = format!("^[^:]+:(//)?([^/]+\\.)?{}", SPECIAL_CHARS.replace_all(&hostname, r##"\$1"##));

                    if v.mask.contains(NetworkFilterMask::IS_HOSTNAME_REGEX) {
                        url_filter += ".*";
                    }

                    url_filter += &with_fixed_wildcards;

                    if v.mask.contains(NetworkFilterMask::IS_RIGHT_ANCHOR) {
                        url_filter += "$";
                    }

                    url_filter
                }
                (crate::filters::network::FilterPart::Simple(part), None) => {
                    let without_trailing_separator = TRAILING_SEPARATOR.replace_all(&part, "");
                    let escaped_special_chars = SPECIAL_CHARS.replace_all(&without_trailing_separator, r##"\$1"##);
                    let with_fixed_wildcards = REPLACE_WILDCARDS.replace_all(&escaped_special_chars, ".*");
                    let mut url_filter = if v.mask.contains(NetworkFilterMask::IS_LEFT_ANCHOR) {
                        format!("^{}", with_fixed_wildcards)
                    } else {
                        let scheme_part = if v.mask.contains(NetworkFilterMask::FROM_HTTP | NetworkFilterMask::FROM_HTTPS) {
                            ""
                        } else if v.mask.contains(NetworkFilterMask::FROM_HTTP) {
                            "^http://.*"
                        } else if v.mask.contains(NetworkFilterMask::FROM_HTTPS) {
                            "^https://.*"
                        } else if v.mask.contains(NetworkFilterMask::FROM_WEBSOCKET) {
                            "^wss?://.*"
                        } else {
                            unreachable!("Invalid scheme information");
                        };

                        format!("{}{}", scheme_part, with_fixed_wildcards)
                    };

                    if v.mask.contains(NetworkFilterMask::IS_RIGHT_ANCHOR) {
                        url_filter += "$";
                    }

                    url_filter
                }
                (crate::filters::network::FilterPart::Empty, Some(hostname)) => {
                    let escaped_special_chars = SPECIAL_CHARS.replace_all(&hostname, r##"\$1"##);
                    format!("^[^:]+:(//)?([^/]+\\.)?{}", escaped_special_chars)
                }
                (crate::filters::network::FilterPart::Empty, None) => {
                    if v.mask.contains(NetworkFilterMask::FROM_HTTP | NetworkFilterMask::FROM_HTTPS) {
                        "^https?://"
                    } else if v.mask.contains(NetworkFilterMask::FROM_HTTP) {
                        "^http://"
                    } else if v.mask.contains(NetworkFilterMask::FROM_HTTPS) {
                        "^https://"
                    } else if v.mask.contains(NetworkFilterMask::FROM_WEBSOCKET) {
                        "^wss?://"
                    } else {
                        unreachable!("Invalid scheme information");
                    }.to_string()
                }
            };

            let (if_domain, unless_domain) = if v.opt_domains.is_some() || v.opt_not_domains.is_some() {
                let mut if_domain = vec![];
                let mut unless_domain = vec![];

                // Unwraps are okay here - any rules with opt_domains or opt_not_domains must have
                // an options section delimited by a '$' character, followed by a `domain=` option.
                let opts = &raw_line[raw_line.find('$').unwrap() + "$".len()..];
                let domains_start = &opts[opts.find("domain=").unwrap() + "domain=".len()..];
                let domains = if let Some(comma) = domains_start.find(',') {
                    &domains_start[..comma]
                } else {
                    domains_start
                }.split('|');

                domains.for_each(|domain| if domain.starts_with('~') {
                        unless_domain.push(format!("*{}", &domain["~".len()..]));
                    } else {
                        if_domain.push(format!("*{}", domain));
                    }
                );

                (non_empty(if_domain), non_empty(unless_domain))
            } else {
                (None, None)
            };

            if if_domain.is_some() && unless_domain.is_some() {
                return Err(CbRuleCreationFailure::UnlessAndIfDomainTogetherUnsupported);
            }

            let blocking_type = if v.mask.contains(NetworkFilterMask::IS_EXCEPTION) {
                CbType::IgnorePreviousRules
            } else {
                CbType::Block
            };

            let resource_type = if v.mask.contains(NetworkFilterMask::FROM_ANY) {
                None
            } else {
                let mut types = std::collections::HashSet::new();
                let mut unsupported_flags = NetworkFilterMask::empty();

                macro_rules! push_if_flag {
                    ($flag:ident, $target:ident) => {
                        if v.mask.contains(NetworkFilterMask::$flag) {
                            types.insert(CbResourceType::$target);
                        }
                    };
                    ($flag:ident) => {
                        if v.mask.contains(NetworkFilterMask::$flag) {
                            unsupported_flags |= NetworkFilterMask::$flag;
                        }
                    };
                }
                push_if_flag!(FROM_IMAGE, Image);
                push_if_flag!(FROM_MEDIA, Media);
                push_if_flag!(FROM_OBJECT);
                push_if_flag!(FROM_OTHER);
                push_if_flag!(FROM_PING);
                push_if_flag!(FROM_SCRIPT, Script);
                push_if_flag!(FROM_STYLESHEET, StyleSheet);
                push_if_flag!(FROM_SUBDOCUMENT, Document);
                push_if_flag!(FROM_WEBSOCKET);
                push_if_flag!(FROM_XMLHTTPREQUEST, Raw);
                push_if_flag!(FROM_FONT, Font);
                // TODO - Popup, Document when implemented

                if !unsupported_flags.is_empty() && types.is_empty() {
                    return Err(CbRuleCreationFailure::NoSupportedNetworkOptions(unsupported_flags));
                }

                Some(types)
            };

            let url_filter_is_case_sensitive = if v.mask.contains(NetworkFilterMask::MATCH_CASE) {
                Some(true)
            } else {
                None
            };


            let single_rule = CbRule {
                action: CbAction { typ: blocking_type, selector: None },
                trigger: CbTrigger {
                    url_filter,
                    load_type,
                    if_domain,
                    unless_domain,
                    resource_type,
                    url_filter_is_case_sensitive,
                    ..Default::default()
                },
            };

            if let Some(resource_types) = &single_rule.trigger.resource_type {
                if resource_types.len() > 1 && resource_types.contains(&CbResourceType::Document) && single_rule.trigger.load_type.is_empty() {
                    let mut non_doc_types = resource_types.clone();
                    non_doc_types.remove(&CbResourceType::Document);
                    let rule_clone = single_rule.clone();
                    let non_doc_rule = CbRule {
                        trigger: CbTrigger {
                            resource_type: Some(non_doc_types),
                            ..rule_clone.trigger
                        },
                        ..rule_clone
                    };
                    let mut doc_type = std::collections::HashSet::new();
                    doc_type.insert(CbResourceType::Document);
                    let just_doc_rule = CbRule {
                        trigger: CbTrigger {
                            resource_type: Some(doc_type),
                            load_type: vec![CbLoadType::ThirdParty],
                            ..single_rule.trigger
                        },
                        ..single_rule
                    };

                    return Ok(Self::SplitDocument(non_doc_rule, just_doc_rule));
                }
            }

            Ok(Self::SingleRule(single_rule))
        }
        else {
            Err(CbRuleCreationFailure::NeedsDebugMode)
        }
    }
}

impl TryFrom<CosmeticFilter> for CbRule {
    type Error = CbRuleCreationFailure;

    fn try_from(v: CosmeticFilter) -> Result<Self, Self::Error> {
        use crate::filters::cosmetic::{CosmeticFilterMask, CosmeticFilterLocationType};

        if v.style.is_some() {
            return Err(CbRuleCreationFailure::CosmeticStyleRulesNotSupported);
        }
        if v.mask.contains(CosmeticFilterMask::SCRIPT_INJECT) {
            return Err(CbRuleCreationFailure::ScriptletInjectionsNotSupported);
        }

        if let Some(raw_line) = v.raw_line {
            let mut hostnames_vec = vec![];
            let mut not_hostnames_vec = vec![];

            let mut any_entities = false;

            // Unwrap is okay here - cosmetic rules must have a '#' character
            let sharp_index = raw_line.find('#').unwrap();
            CosmeticFilter::locations_before_sharp(&raw_line, sharp_index).for_each(|(location_type, location)| {
                match location_type {
                    CosmeticFilterLocationType::Entity => any_entities = true,
                    CosmeticFilterLocationType::NotEntity => any_entities = true,
                    CosmeticFilterLocationType::Hostname => hostnames_vec.push(location.to_string()),
                    CosmeticFilterLocationType::NotHostname => not_hostnames_vec.push(location.to_string()),
                }
            });

            if any_entities {
                return Err(CbRuleCreationFailure::CosmeticEntitiesUnsupported);
            }

            let hostnames_vec = non_empty(hostnames_vec);
            let not_hostnames_vec = non_empty(not_hostnames_vec);

            if hostnames_vec.is_some() && not_hostnames_vec.is_some() {
                return Err(CbRuleCreationFailure::UnlessAndIfDomainTogetherUnsupported);
            }

            let (unless_domain, if_domain) = if v.mask.contains(CosmeticFilterMask::UNHIDE) {
                (hostnames_vec, not_hostnames_vec)
            } else {
                (not_hostnames_vec, hostnames_vec)
            };

            Ok(Self {
                action: CbAction { typ: CbType::CssDisplayNone, selector: Some(v.selector) },
                trigger: CbTrigger {
                    url_filter: ".*".to_string(),
                    if_domain,
                    unless_domain,
                    ..Default::default()
                },
            })
        } else {
            Err(CbRuleCreationFailure::NeedsDebugMode)
        }
    }
}

#[cfg(test)]
mod ab2cb_tests {
    use super::*;
    use crate::lists::FilterFormat;

    fn test_from_abp(abp_rule: &str, cb: &str) {
        let filter = crate::lists::parse_filter(abp_rule, true, FilterFormat::Standard).expect("Rule under test could not be parsed");
        assert_eq!(CbRuleEquivalent::try_from(filter).unwrap().into_iter().collect::<Vec<_>>(), serde_json::from_str::<Vec<CbRule>>(cb).expect("content blocking rule under test could not be deserialized"));
    }

    #[test]
    fn ad_tests() {
        test_from_abp("&ad_box_", r####"[{
            "action": {
                "type": "block"
            },
            "trigger": {
                "url-filter": "&ad_box_"
            }
        }]"####);
        test_from_abp("&ad_channel=", r####"[{
            "action": {
                "type": "block"
            },
            "trigger": {
                "url-filter": "&ad_channel="
            }
        }]"####);
        test_from_abp("+advertorial.", r####"[{
            "action": {
                "type": "block"
            },
            "trigger": {
                "url-filter": "\\+advertorial\\."
            }
        }]"####);
        test_from_abp("&prvtof=*&poru=", r####"[{
            "action": {
                "type": "block"
            },
            "trigger": {
                "url-filter": "&prvtof=.*&poru="
            }
        }]"####);
        test_from_abp("-ad-180x150px.", r####"[{
            "action": {
                "type": "block"
            },
            "trigger": {
                "url-filter": "-ad-180x150px\\."
            }
        }]"####);
        test_from_abp("://findnsave.*.*/api/groupon.json?", r####"[{
            "action": {
                "type": "block"
            },
            "trigger": {
                "url-filter": "://findnsave\\..*\\..*/api/groupon\\.json\\?"
            }
        }]"####);
        test_from_abp("|https://$script,third-party,domain=tamilrockers.ws", r####"[{
            "action": {
                "type": "block"
            },
            "trigger": {
                "if-domain": ["*tamilrockers.ws"],
                "load-type": ["third-party"],
                "resource-type": ["script"],
                "url-filter": "^https://"
            }
        }]"####);
        test_from_abp("||com/banners/$image,object,subdocument,domain=~pingdom.com|~thetvdb.com|~tooltrucks.com", r####"[{
            "action": {
                "type": "block"
            },
            "trigger": {
                "url-filter": "^[^:]+:(//)?([^/]+\\.)?com/banners/",
                "unless-domain": [
                    "*pingdom.com",
                    "*thetvdb.com",
                    "*tooltrucks.com"
                ],
                "resource-type": [
                    "image"
                ]
            }
        }, {
            "trigger": {
                "url-filter": "^[^:]+:(//)?([^/]+\\.)?com/banners/",
                "unless-domain": [
                    "*pingdom.com",
                    "*thetvdb.com",
                    "*tooltrucks.com"
                ],
                "resource-type": [
                    "document"
                ],
                "load-type": [
                    "third-party"
                ]
            },
            "action": {
                "type": "block"
            }
        }]"####);
        test_from_abp("$image,third-party,xmlhttprequest,domain=rd.com", r####"[{
            "action": {
                "type": "block"
            },
            "trigger": {
                "url-filter": "^https?://",
                "if-domain": [
                    "*rd.com"
                ],
                "resource-type": [
                    "image",
                    "raw"
                ],
                "load-type": [
                    "third-party"
                ]
            }
        }]"####);
        test_from_abp("|https://r.i.ua^", r####"[{
            "action": {
                "type": "block"
            },
            "trigger": {
                "url-filter": "^https://r\\.i\\.ua"
            }
        }]"####);
        test_from_abp("|ws://$domain=4shared.com", r####"[{
            "action": {
                "type": "block"
            },
            "trigger": {
                "url-filter": "^wss?://",
                "if-domain": [
                    "*4shared.com"
                ]
            }
        }]"####);
    }

    #[test]
    fn element_hiding_tests() {
        test_from_abp("###A9AdsMiddleBoxTop", r####"[{
            "action": {
                "type": "css-display-none",
                "selector": "#A9AdsMiddleBoxTop"
            },
            "trigger": {
                "url-filter": ".*"
            }
        }]"####);
        test_from_abp("thedailygreen.com#@##AD_banner", r####"[{
            "action": {
                "type": "css-display-none",
                "selector": "#AD_banner"
            },
            "trigger": {
                "url-filter": ".*",
                "unless-domain": [
                    "thedailygreen.com"
                ]
            }
        }]"####);
        test_from_abp("sprouts.com,tbns.com.au#@##AdImage", r####"[{
            "action": {
                "type": "css-display-none",
                "selector": "#AdImage"
            },
            "trigger": {
                "url-filter": ".*",
                "unless-domain": [
                    "sprouts.com",
                    "tbns.com.au"
                ]
            }
        }]"####);
        test_from_abp(r#"santander.co.uk#@#a[href^="http://ad-emea.doubleclick.net/"]"#, r####"[{
            "action": {
                "type": "css-display-none",
                "selector": "a[href^=\"http://ad-emea.doubleclick.net/\"]"
            },
            "trigger": {
                "url-filter": ".*",
                "unless-domain": [
                    "santander.co.uk"
                ]
            }
        }]"####);
        test_from_abp("search.safefinder.com,search.snapdo.com###ABottomD", r####"[{
            "action": {
                "type": "css-display-none",
                "selector": "#ABottomD"
            },
            "trigger": {
                "url-filter": ".*",
                "if-domain": [
                    "search.safefinder.com",
                    "search.snapdo.com"
                ]
            }
        }]"####);
        test_from_abp(r#"tweakguides.com###adbar > br + p[style="text-align: center"] + p[style="text-align: center"]"#, r####"[{
            "action": {
                "type": "css-display-none",
                "selector": "#adbar > br + p[style=\"text-align: center\"] + p[style=\"text-align: center\"]"
            },
            "trigger": {
                "url-filter": ".*",
                "if-domain": [
                    "tweakguides.com"
                ]
            }
        }]"####);
    }

    /* TODO - `$popup` is currently unsupported by NetworkFilter
    #[test]
    fn popup_tests() {
        test_from_abp("||admngronline.com^$popup,third-party", r####"[{
            "action": {
                "type": "block"
            },
            "trigger": {
                "url-filter": "^https?://admngronline\\.com(?:[\\x00-\\x24\\x26-\\x2C\\x2F\\x3A-\\x40\\x5B-\\x5E\\x60\\x7B-\\x7F]|$)",
                "load-type": [
                    "third-party"
                ],
                "resource-type": [
                    "popup"
                ]
            }
        }]"####);
        test_from_abp("||bet365.com^*affiliate=$popup", r####"[{
            "action": {
                "type": "block"
            },
            "trigger": {
                "url-filter": "^https?://bet365\\.com(?:[\\x00-\\x24\\x26-\\x2C\\x2F\\x3A-\\x40\\x5B-\\x5E\\x60\\x7B-\\x7F]|$).*affiliate=",
                "resource-type": [
                    "popup"
                ]
            }
        }]"####);
    }
    */

    #[test]
    fn third_party() {
        test_from_abp("||007-gateway.com^$third-party", r####"[{
            "action": {
                "type": "block"
            },
            "trigger": {
                "url-filter": "^[^:]+:(//)?([^/]+\\.)?007-gateway\\.com",
                "load-type": [
                    "third-party"
                ]
            }
        }]"####);
        test_from_abp("||anet*.tradedoubler.com^$third-party", r####"[{
            "action": {
                "type": "block"
            },
            "trigger": {
                "url-filter": "^[^:]+:(//)?([^/]+\\.)?anet.*\\.tradedoubler\\.com",
                "load-type": [
                    "third-party"
                ]
            }
        }]"####);
        test_from_abp("||doubleclick.net^$third-party,domain=3news.co.nz|92q.com|abc-7.com|addictinggames.com|allbusiness.com|allthingsd.com|bizjournals.com|bloomberg.com|bnn.ca|boom92houston.com|boom945.com|boomphilly.com|break.com|cbc.ca|cbs19.tv|cbs3springfield.com|cbsatlanta.com|cbslocal.com|complex.com|dailymail.co.uk|darkhorizons.com|doubleviking.com|euronews.com|extratv.com|fandango.com|fox19.com|fox5vegas.com|gorillanation.com|hawaiinewsnow.com|hellobeautiful.com|hiphopnc.com|hot1041stl.com|hothiphopdetroit.com|hotspotatl.com|hulu.com|imdb.com|indiatimes.com|indyhiphop.com|ipowerrichmond.com|joblo.com|kcra.com|kctv5.com|ketv.com|koat.com|koco.com|kolotv.com|kpho.com|kptv.com|ksat.com|ksbw.com|ksfy.com|ksl.com|kypost.com|kysdc.com|live5news.com|livestation.com|livestream.com|metro.us|metronews.ca|miamiherald.com|my9nj.com|myboom1029.com|mycolumbusmagic.com|mycolumbuspower.com|myfoxdetroit.com|myfoxorlando.com|myfoxphilly.com|myfoxphoenix.com|myfoxtampabay.com|nbcrightnow.com|neatorama.com|necn.com|neopets.com|news.com.au|news4jax.com|newsone.com|nintendoeverything.com|oldschoolcincy.com|own3d.tv|pagesuite-professional.co.uk|pandora.com|player.theplatform.com|ps3news.com|radio.com|radionowindy.com|rottentomatoes.com|sbsun.com|shacknews.com|sk-gaming.com|ted.com|thebeatdfw.com|theboxhouston.com|theglobeandmail.com|timesnow.tv|tv2.no|twitch.tv|universalsports.com|ustream.tv|wapt.com|washingtonpost.com|wate.com|wbaltv.com|wcvb.com|wdrb.com|wdsu.com|wflx.com|wfmz.com|wfsb.com|wgal.com|whdh.com|wired.com|wisn.com|wiznation.com|wlky.com|wlns.com|wlwt.com|wmur.com|wnem.com|wowt.com|wral.com|wsj.com|wsmv.com|wsvn.com|wtae.com|wthr.com|wxii12.com|wyff4.com|yahoo.com|youtube.com|zhiphopcleveland.com", r####"[{
            "action": {
                "type": "block"
            },
            "trigger": {
                "url-filter": "^[^:]+:(//)?([^/]+\\.)?doubleclick\\.net",
                "load-type": [
                    "third-party"
                ],
                "if-domain": [
                    "*3news.co.nz",
                    "*92q.com",
                    "*abc-7.com",
                    "*addictinggames.com",
                    "*allbusiness.com",
                    "*allthingsd.com",
                    "*bizjournals.com",
                    "*bloomberg.com",
                    "*bnn.ca",
                    "*boom92houston.com",
                    "*boom945.com",
                    "*boomphilly.com",
                    "*break.com",
                    "*cbc.ca",
                    "*cbs19.tv",
                    "*cbs3springfield.com",
                    "*cbsatlanta.com",
                    "*cbslocal.com",
                    "*complex.com",
                    "*dailymail.co.uk",
                    "*darkhorizons.com",
                    "*doubleviking.com",
                    "*euronews.com",
                    "*extratv.com",
                    "*fandango.com",
                    "*fox19.com",
                    "*fox5vegas.com",
                    "*gorillanation.com",
                    "*hawaiinewsnow.com",
                    "*hellobeautiful.com",
                    "*hiphopnc.com",
                    "*hot1041stl.com",
                    "*hothiphopdetroit.com",
                    "*hotspotatl.com",
                    "*hulu.com",
                    "*imdb.com",
                    "*indiatimes.com",
                    "*indyhiphop.com",
                    "*ipowerrichmond.com",
                    "*joblo.com",
                    "*kcra.com",
                    "*kctv5.com",
                    "*ketv.com",
                    "*koat.com",
                    "*koco.com",
                    "*kolotv.com",
                    "*kpho.com",
                    "*kptv.com",
                    "*ksat.com",
                    "*ksbw.com",
                    "*ksfy.com",
                    "*ksl.com",
                    "*kypost.com",
                    "*kysdc.com",
                    "*live5news.com",
                    "*livestation.com",
                    "*livestream.com",
                    "*metro.us",
                    "*metronews.ca",
                    "*miamiherald.com",
                    "*my9nj.com",
                    "*myboom1029.com",
                    "*mycolumbusmagic.com",
                    "*mycolumbuspower.com",
                    "*myfoxdetroit.com",
                    "*myfoxorlando.com",
                    "*myfoxphilly.com",
                    "*myfoxphoenix.com",
                    "*myfoxtampabay.com",
                    "*nbcrightnow.com",
                    "*neatorama.com",
                    "*necn.com",
                    "*neopets.com",
                    "*news.com.au",
                    "*news4jax.com",
                    "*newsone.com",
                    "*nintendoeverything.com",
                    "*oldschoolcincy.com",
                    "*own3d.tv",
                    "*pagesuite-professional.co.uk",
                    "*pandora.com",
                    "*player.theplatform.com",
                    "*ps3news.com",
                    "*radio.com",
                    "*radionowindy.com",
                    "*rottentomatoes.com",
                    "*sbsun.com",
                    "*shacknews.com",
                    "*sk-gaming.com",
                    "*ted.com",
                    "*thebeatdfw.com",
                    "*theboxhouston.com",
                    "*theglobeandmail.com",
                    "*timesnow.tv",
                    "*tv2.no",
                    "*twitch.tv",
                    "*universalsports.com",
                    "*ustream.tv",
                    "*wapt.com",
                    "*washingtonpost.com",
                    "*wate.com",
                    "*wbaltv.com",
                    "*wcvb.com",
                    "*wdrb.com",
                    "*wdsu.com",
                    "*wflx.com",
                    "*wfmz.com",
                    "*wfsb.com",
                    "*wgal.com",
                    "*whdh.com",
                    "*wired.com",
                    "*wisn.com",
                    "*wiznation.com",
                    "*wlky.com",
                    "*wlns.com",
                    "*wlwt.com",
                    "*wmur.com",
                    "*wnem.com",
                    "*wowt.com",
                    "*wral.com",
                    "*wsj.com",
                    "*wsmv.com",
                    "*wsvn.com",
                    "*wtae.com",
                    "*wthr.com",
                    "*wxii12.com",
                    "*wyff4.com",
                    "*yahoo.com",
                    "*youtube.com",
                    "*zhiphopcleveland.com"
                ]
            }
        }]"####);
        test_from_abp("||dt00.net^$third-party,domain=~marketgid.com|~marketgid.ru|~marketgid.ua|~mgid.com|~thechive.com", r####"[{
            "action": {
                "type": "block"
            },
            "trigger": {
                "url-filter": "^[^:]+:(//)?([^/]+\\.)?dt00\\.net",
                "load-type": [
                    "third-party"
                ],
                "unless-domain": [
                    "*marketgid.com",
                    "*marketgid.ru",
                    "*marketgid.ua",
                    "*mgid.com",
                    "*thechive.com"
                ]
            }
        }]"####);
        test_from_abp("||amazonaws.com/newscloud-production/*/backgrounds/$domain=crescent-news.com|daily-jeff.com|recordpub.com|state-journal.com|the-daily-record.com|the-review.com|times-gazette.com", r####"[{
            "action": {
                "type": "block"
            },
            "trigger": {
                "url-filter": "^[^:]+:(//)?([^/]+\\.)?amazonaws\\.com/newscloud-production/.*/backgrounds/",
                "if-domain": [
                    "*crescent-news.com",
                    "*daily-jeff.com",
                    "*recordpub.com",
                    "*state-journal.com",
                    "*the-daily-record.com",
                    "*the-review.com",
                    "*times-gazette.com"
                ]
            }
        }]"####);
        test_from_abp("||d1noellhv8fksc.cloudfront.net^", r####"[{
            "action": {
                "type": "block"
            },
            "trigger": {
                "url-filter": "^[^:]+:(//)?([^/]+\\.)?d1noellhv8fksc\\.cloudfront\\.net"
            }
        }]"####);
    }

    #[test]
    fn whitelist() {
        test_from_abp("@@||google.com/recaptcha/$domain=mediafire.com", r####"[{
            "action": {
                "type": "ignore-previous-rules"
            },
            "trigger": {
                "url-filter": "^[^:]+:(//)?([^/]+\\.)?google\\.com/recaptcha/",
                "if-domain": [
                    "*mediafire.com"
                ]
            }
        }]"####);
        test_from_abp("@@||ad4.liverail.com/?compressed|$domain=majorleaguegaming.com|pbs.org|wikihow.com", r####"[{
            "action": {
                "type": "ignore-previous-rules"
            },
            "trigger": {
                "url-filter": "^[^:]+:(//)?([^/]+\\.)?ad4\\.liverail\\.com/\\?compressed$",
                "if-domain": [
                    "*majorleaguegaming.com",
                    "*pbs.org",
                    "*wikihow.com"
                ]
            }
        }]"####);
        test_from_abp("@@||advertising.autotrader.co.uk^$~third-party", r####"[{
            "action": {
                "type": "ignore-previous-rules"
            },
            "trigger": {
                "load-type": [
                    "first-party"
                ],
                "url-filter": "^[^:]+:(//)?([^/]+\\.)?advertising\\.autotrader\\.co\\.uk"
            }
        }]"####);
        test_from_abp("@@||advertising.racingpost.com^$image,script,stylesheet,~third-party,xmlhttprequest", r####"[{
            "action": {
                "type": "ignore-previous-rules"
            },
            "trigger": {
                "load-type": [
                    "first-party"
                ],
                "url-filter": "^[^:]+:(//)?([^/]+\\.)?advertising\\.racingpost\\.com",
                "resource-type": [
                    "image",
                    "style-sheet",
                    "script",
                    "raw"
                ]
            }
        }]"####);
    }
}
