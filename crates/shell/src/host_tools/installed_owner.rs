//! Pure owner selection from admitted declarations. No I/O and no grants.
//! Native/system namespace precedence stays in Catalog::owner_of.
#[derive(Debug, PartialEq, Eq)]
pub enum Resolution { Missing, Unique(String), Ambiguous }

pub fn resolve<'a>(namespace: &str, entries: impl IntoIterator<Item = (&'a str, bool)>) -> Resolution {
    let mut found: Option<&str> = None;
    for (app, declares_exact_tool) in entries {
        if !declares_exact_tool || app.starts_with("os.") || app.rsplit('.').next().unwrap_or(app).replace('-', "") != namespace {
            continue;
        }
        if let Some(previous) = found {
            if previous != app { return Resolution::Ambiguous; }
        }
        found = Some(app);
    }
    match found { Some(app) => Resolution::Unique(app.to_owned()), None => Resolution::Missing }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn hyphenated_identity_stays_intact() {
        assert_eq!(resolve("bookledger", [("book-ledger",true)]),Resolution::Unique("book-ledger".into()));
        assert_eq!(resolve("bookledger", [("book-ledger",true),("bookledger",true)]),Resolution::Ambiguous);
    }
    #[test] fn admitted_flat_app_is_not_rewritten_to_os() {
        assert_eq!(resolve("ledger", [("ledger",true)]),Resolution::Unique("ledger".into()));
    }
    #[test] fn reverse_domain_identity_preserved() {
        assert_eq!(resolve("ledger", [("org.example.ledger",true)]),Resolution::Unique("org.example.ledger".into()));
    }
    #[test] fn ambiguous_namespace_is_never_first_wins() {
        assert_eq!(resolve("ledger", [("a.ledger",true),("b.ledger",true)]),Resolution::Ambiguous);
        assert_eq!(resolve("ledger", [("b.ledger",true),("a.ledger",true)]),Resolution::Ambiguous);
    }
    #[test] fn spoofed_namespace_is_not_an_owner() {
        assert_eq!(resolve("mail", [("evil",true)]),Resolution::Missing);
    }
    #[test] fn exact_tool_must_be_declared() {
        assert_eq!(resolve("ledger", [("ledger",false)]),Resolution::Missing);
    }
    #[test] fn system_namespace_remains_separate() {
        assert_eq!(resolve("mail", [("os.mail",true)]),Resolution::Missing);
    }
}
