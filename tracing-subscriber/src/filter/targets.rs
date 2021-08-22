use crate::{
    filter::{
        directive::{DirectiveParseError, DirectiveSet, StaticDirective},
        LevelFilter,
    },
    layer,
};
use std::{
    iter::{Extend, FromIterator},
    str::FromStr,
};
use tracing_core::{Interest, Metadata, Subscriber};

/// A filter that enables or disables spans and events based on their [target]
/// and [level].
///
/// Targets are typically equal to the Rust module path of the code where the
/// span or event was recorded, although they may be overridden.
///
/// This type can be used for both [per-layer filtering][plf] (using its
/// [`Filter`] implementation) and [global filtering][global] (using its
/// [`Layer`] implementation).
///
/// See the [documentation on filtering with layers][filtering] for details.
///
/// # Filtering With `Targets`
///
/// A `Targets` filter consists of one or more [target] prefixes, paired with
/// [`LevelFilter`]s. If a span or event's [target] begins with one of those
/// prefixes, and its [level] is at or below the [`LevelFilter`] enabled for
/// that prefix, then the span or event will be enabled.
///
/// This is similar to the behavior implemented by the [`env_logger` crate] in
/// the `log` ecosystem.
///
/// The [`EnvFilter`] type also provided by this crate is very similar to `Targets`,
/// but is capable of a more sophisticated form of filtering where events may
/// also be enabled or disabled based on the span they are recorded in.
/// `Targets` can be thought of as a lighter-weight form of [`EnvFilter`] that
/// can be used instead when this dynamic filtering is not required.
///
/// # Examples
///
/// A `Targets` filter can be constructed by programmatically adding targets and
/// levels to enable:
///
/// ```
/// use tracing_subscriber::{filter, prelude::*};
/// use tracing_core::Level;
///
/// let filter = filter::Targets::new()
///     // Enable the `INFO` level for anything in `my_crate`
///     .with_target("my_crate", Level::INFO)
///     // Enable the `DEBUG` level for a specific module.
///     .with_target("my_crate::interesting_module", Level::DEBUG);
///
/// // Build a new subscriber with the `fmt` layer using the `Targets`
/// // filter we constructed above.
/// tracing_subscriber::registry()
///     .with(tracing_subscriber::fmt::layer())
///     .with(filter)
///     .init();
/// ```
///
/// [`LevelFilter::OFF`] can be used to disable a particular target:
/// ```
/// use tracing_subscriber::filter::{Targets, LevelFilter};
/// use tracing_core::Level;
///
/// let filter = Targets::new()
///     .with_target("my_crate", Level::INFO)
///     // Disable all traces from `annoying_module`.
///     .with_target("my_crate::annoying_module", LevelFilter::OFF);
/// # drop(filter);
/// ```
///
/// Alternatively, `Targets` implements [`std::str::FromStr`], allowing it to be
/// parsed from a comma-delimited list of `target=level` pairs. For example:
///
/// ```rust
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// use tracing_subscriber::filter;
/// use tracing_core::Level;
///
/// let filter = "my_crate=info,my_crate::interesting_module=trace,other_crate=debug"
///     .parse::<filter::Targets>()?;
///
/// // The parsed filter is identical to a filter constructed using `with_target`:
/// assert_eq!(
///     filter,
///     filter::Targets::new()
///         .with_target("my_crate", Level::INFO)
///         .with_target("my_crate::interesting_module", Level::TRACE)
///         .with_target("other_crate", Level::DEBUG)
/// );
/// # Ok(()) }
/// ```
///
/// This is particularly useful when the list of enabled targets is configurable
/// by the user at runtime.
///
/// The `Targets` filter can be used as a [per-layer filter][plf] *and* as a
/// [global filter]:
///
/// ```rust
/// use tracing_subscriber::{
///     fmt,
///     filter::{Targets, LevelFilter},
///     prelude::*,
/// };
/// use tracing_core::Level;
/// use std::{sync::Arc, fs::File};
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
///
/// // A layer that logs events to stdout using the human-readable "pretty"
/// // format.
/// let stdout_log = fmt::layer().pretty();
///
/// // A layer that logs events to a file, using the JSON format.
/// let file = File::create("debug_log.json")?;
/// let debug_log = fmt::layer()
///     .with_writer(Arc::new(file))
///     .json();
///
/// tracing_subscriber::registry()
///     // Only log INFO and above to stdout, unless the span or event
///     // has the `my_crate::cool_module` target prefix.
///     .with(stdout_log
///         .with_filter(
///             Targets::default()
///                 .with_target("my_crate::cool_module", Level::DEBUG)
///                 .with_default(Level::INFO)
///        )
///     )
///     // Log everything enabled by the global filter to `debug_log.json`.
///     .with(debug_log)
///     // Configure a global filter for the whole subscriber stack. This will
///     // control what spans and events are recorded by both the `debug_log`
///     // and the `stdout_log` layers, and `stdout_log` will *additionally* be
///     // filtered by its per-layer filter.
///     .with(
///         Targets::default()
///             .with_target("my_crate", Level::TRACE)
///             .with_target("other_crate", Level::INFO)
///             .with_target("other_crate::annoying_module", LevelFilter::OFF)
///             .with_target("third_crate", Level::DEBUG)
///     ).init();
/// # Ok(()) }
///```
///
/// [target]: tracing_core::Metadata::target
/// [level]: tracing_core::Level
/// [`Filter`]: crate::layer::Filter
/// [`Layer`]: crate::layer::Layer
/// [plf]: crate::layer#per-layer-filtering
/// [global]: crate::layer#global-filtering
/// [filtering]: crate::layer#filtering-with-layers
/// [`env_logger` crate]: https://docs.rs/env_logger/0.9.0/env_logger/index.html#enabling-logging
/// [`EnvFilter`]: crate::filter::EnvFilter
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Targets(DirectiveSet<StaticDirective>);

impl Targets {
    /// Returns a new `Targets` filter.
    ///
    /// This filter will enable no targets. Call [`with_target`] or [`with_targets`]
    /// to add enabled targets, and [`with_default`] to change the default level
    /// enabled for spans and events that didn't match any of the provided targets.
    ///
    /// [`with_target`]: Targets::with_target
    /// [`with_targets`]: Targets::with_targets
    /// [`with_default`]: Targets::with_default
    pub fn new() -> Self {
        Self::default()
    }

    /// Enables spans and events with [target]s starting with the provided target
    /// prefix if they are at or below the provided [`LevelFilter`].
    ///
    /// # Examples
    ///
    /// ```
    /// use tracing_subscriber::filter;
    /// use tracing_core::Level;
    ///
    /// let filter = filter::Targets::new()
    ///     // Enable the `INFO` level for anything in `my_crate`
    ///     .with_target("my_crate", Level::INFO)
    ///     // Enable the `DEBUG` level for a specific module.
    ///     .with_target("my_crate::interesting_module", Level::DEBUG);
    /// # drop(filter);
    /// ```
    ///
    /// [`LevelFilter::OFF`] can be used to disable a particular target:
    /// ```
    /// use tracing_subscriber::filter::{Targets, LevelFilter};
    /// use tracing_core::Level;
    ///
    /// let filter = Targets::new()
    ///     .with_target("my_crate", Level::INFO)
    ///     // Disable all traces from `annoying_module`.
    ///     .with_target("my_crate::interesting_module", LevelFilter::OFF);
    /// # drop(filter);
    /// ```
    ///
    /// [target]: tracing_core::Metadata::target
    pub fn with_target(mut self, target: impl Into<String>, level: impl Into<LevelFilter>) -> Self {
        self.0.add(StaticDirective::new(
            Some(target.into()),
            Default::default(),
            level.into(),
        ));
        self
    }
    /// Adds [target]s from an iterator of [target]-[`LevelFilter`] pairs to this filter.
    ///
    /// # Examples
    ///
    /// ```
    /// use tracing_subscriber::filter;
    /// use tracing_core::Level;
    ///
    /// let filter = filter::Targets::new()
    ///     .with_targets(vec![
    ///         ("my_crate", Level::INFO),
    ///         ("my_crate::some_module", Level::DEBUG),
    ///         ("my_crate::other_module::cool_stuff", Level::TRACE),
    ///         ("other_crate", Level::WARN)
    ///     ]);
    /// # drop(filter);
    /// ```
    ///
    /// [`LevelFilter::OFF`] can be used to disable a particular target:
    /// ```
    /// use tracing_subscriber::filter::{Targets, LevelFilter};
    /// use tracing_core::Level;
    ///
    /// let filter = Targets::new()
    ///     .with_target("my_crate", Level::INFO)
    ///     // Disable all traces from `annoying_module`.
    ///     .with_target("my_crate::interesting_module", LevelFilter::OFF);
    /// # drop(filter);
    /// ```
    ///
    /// [target]: tracing_core::Metadata::target
    pub fn with_targets<T, L>(mut self, targets: impl IntoIterator<Item = (T, L)>) -> Self
    where
        String: From<T>,
        LevelFilter: From<L>,
    {
        self.extend(targets);
        self
    }

    /// Sets the default level to enable for spans and events whose targets did
    /// not match any of the configured prefixes.
    ///
    /// By default, this is [`LevelFilter::OFF`]. This means that spans and
    /// events will only be enabled if they match one of the configured target
    /// prefixes. If this is changed to a different [`LevelFilter`], spans and
    /// events with targets that did not match any of the configured prefixes
    /// will be enabled if their level is at or below the provided level.
    pub fn with_default(mut self, level: impl Into<LevelFilter>) -> Self {
        self.0
            .add(StaticDirective::new(None, Default::default(), level.into()));
        self
    }

    #[inline]
    fn interested(&self, metadata: &'static Metadata<'static>) -> Interest {
        if self.0.enabled(metadata) {
            Interest::always()
        } else {
            Interest::never()
        }
    }
}

impl<T, L> Extend<(T, L)> for Targets
where
    T: Into<String>,
    L: Into<LevelFilter>,
{
    fn extend<I: IntoIterator<Item = (T, L)>>(&mut self, iter: I) {
        let iter = iter.into_iter().map(|(target, level)| {
            StaticDirective::new(Some(target.into()), Default::default(), level.into())
        });
        self.0.extend(iter);
    }
}

impl<T, L> FromIterator<(T, L)> for Targets
where
    T: Into<String>,
    L: Into<LevelFilter>,
{
    fn from_iter<I: IntoIterator<Item = (T, L)>>(iter: I) -> Self {
        let mut this = Self::default();
        this.extend(iter);
        this
    }
}

impl FromStr for Targets {
    type Err = DirectiveParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.split(',')
            .map(StaticDirective::from_str)
            .collect::<Result<_, _>>()
            .map(Self)
    }
}

impl<S> layer::Layer<S> for Targets
where
    S: Subscriber,
{
    fn enabled(&self, metadata: &Metadata<'_>, _: layer::Context<'_, S>) -> bool {
        self.0.enabled(metadata)
    }

    fn register_callsite(&self, metadata: &'static Metadata<'static>) -> Interest {
        self.interested(metadata)
    }

    fn max_level_hint(&self) -> Option<LevelFilter> {
        Some(self.0.max_level)
    }
}

#[cfg(feature = "registry")]
#[cfg_attr(docsrs, doc(cfg(feature = "registry")))]
impl<S> layer::Filter<S> for Targets {
    fn enabled(&self, metadata: &Metadata<'_>, _: &layer::Context<'_, S>) -> bool {
        self.0.enabled(metadata)
    }

    fn callsite_enabled(&self, metadata: &'static Metadata<'static>) -> Interest {
        self.interested(metadata)
    }

    fn max_level_hint(&self) -> Option<LevelFilter> {
        Some(self.0.max_level)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filter::directive::FilterVec;

    fn expect_parse(s: &str) -> FilterVec<StaticDirective> {
        match dbg!(s).parse::<Targets>() {
            Err(e) => panic!("string {:?} did not parse successfully: {}", s, e),
            Ok(e) => e.0.into_vec(),
        }
    }

    fn expect_parse_ralith(s: &str) {
        let dirs = expect_parse(s);
        assert_eq!(dirs.len(), 2, "\nparsed: {:#?}", dirs);
        assert_eq!(dirs[0].target, Some("server".to_string()));
        assert_eq!(dirs[0].level, LevelFilter::DEBUG);
        assert_eq!(dirs[0].field_names, FilterVec::<String>::default());

        assert_eq!(dirs[1].target, Some("common".to_string()));
        assert_eq!(dirs[1].level, LevelFilter::INFO);
        assert_eq!(dirs[1].field_names, FilterVec::<String>::default());
    }

    fn expect_parse_level_directives(s: &str) {
        let dirs = expect_parse(s);
        assert_eq!(dirs.len(), 6, "\nparsed: {:#?}", dirs);

        assert_eq!(dirs[0].target, Some("crate3::mod2::mod1".to_string()));
        assert_eq!(dirs[0].level, LevelFilter::OFF);
        assert_eq!(dirs[0].field_names, FilterVec::<String>::default());

        assert_eq!(dirs[1].target, Some("crate1::mod2::mod3".to_string()));
        assert_eq!(dirs[1].level, LevelFilter::INFO);
        assert_eq!(dirs[1].field_names, FilterVec::<String>::default());

        assert_eq!(dirs[2].target, Some("crate1::mod2".to_string()));
        assert_eq!(dirs[2].level, LevelFilter::WARN);
        assert_eq!(dirs[2].field_names, FilterVec::<String>::default());

        assert_eq!(dirs[3].target, Some("crate1::mod1".to_string()));
        assert_eq!(dirs[3].level, LevelFilter::ERROR);
        assert_eq!(dirs[3].field_names, FilterVec::<String>::default());

        assert_eq!(dirs[4].target, Some("crate3".to_string()));
        assert_eq!(dirs[4].level, LevelFilter::TRACE);
        assert_eq!(dirs[4].field_names, FilterVec::<String>::default());

        assert_eq!(dirs[5].target, Some("crate2".to_string()));
        assert_eq!(dirs[5].level, LevelFilter::DEBUG);
        assert_eq!(dirs[5].field_names, FilterVec::<String>::default());
    }

    #[test]
    fn parse_ralith() {
        expect_parse_ralith("common=info,server=debug");
    }

    #[test]
    fn parse_ralith_uc() {
        expect_parse_ralith("common=INFO,server=DEBUG");
    }

    #[test]
    fn parse_ralith_mixed() {
        expect_parse("common=iNfo,server=dEbUg");
    }

    #[test]
    fn expect_parse_valid() {
        let dirs = expect_parse("crate1::mod1=error,crate1::mod2,crate2=debug,crate3=off");
        assert_eq!(dirs.len(), 4, "\nparsed: {:#?}", dirs);
        assert_eq!(dirs[0].target, Some("crate1::mod2".to_string()));
        assert_eq!(dirs[0].level, LevelFilter::TRACE);
        assert_eq!(dirs[0].field_names, FilterVec::<String>::default());

        assert_eq!(dirs[1].target, Some("crate1::mod1".to_string()));
        assert_eq!(dirs[1].level, LevelFilter::ERROR);
        assert_eq!(dirs[1].field_names, FilterVec::<String>::default());

        assert_eq!(dirs[2].target, Some("crate3".to_string()));
        assert_eq!(dirs[2].level, LevelFilter::OFF);
        assert_eq!(dirs[2].field_names, FilterVec::<String>::default());

        assert_eq!(dirs[3].target, Some("crate2".to_string()));
        assert_eq!(dirs[3].level, LevelFilter::DEBUG);
        assert_eq!(dirs[3].field_names, FilterVec::<String>::default());
    }

    #[test]
    fn parse_level_directives() {
        expect_parse_level_directives(
            "crate1::mod1=error,crate1::mod2=warn,crate1::mod2::mod3=info,\
             crate2=debug,crate3=trace,crate3::mod2::mod1=off",
        )
    }

    #[test]
    fn parse_uppercase_level_directives() {
        expect_parse_level_directives(
            "crate1::mod1=ERROR,crate1::mod2=WARN,crate1::mod2::mod3=INFO,\
             crate2=DEBUG,crate3=TRACE,crate3::mod2::mod1=OFF",
        )
    }

    #[test]
    fn parse_numeric_level_directives() {
        expect_parse_level_directives(
            "crate1::mod1=1,crate1::mod2=2,crate1::mod2::mod3=3,crate2=4,\
             crate3=5,crate3::mod2::mod1=0",
        )
    }
}