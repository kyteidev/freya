//! # Global Signals
//!
//! [Global Signals](dioxus::prelude::GlobalSignal) are like [Signals](dioxus::prelude::Signal) but you declare them statically and you don't need to pass them through props or context as you can just import it.
//!
//! **Main use case is for apps, not libraries.**
//!
//! ### Example
//!
//! ```rust
//! # use freya::prelude::*;
//! static COUNT: GlobalSignal<usize> = Signal::global(|| 0);
//!
//! fn app() -> Element {
//!     let onclick = move |_| {
//!         *COUNT.write() += 1; // Modify the global signal, as if it was a normal signal
//!     };
//!
//!     rsx!(
//!         label {
//!             onclick,
//!             "{COUNT}" // Read the global signal
//!         }
//!         SomeOtherComp {}
//!     )
//! }
//!
//! #[component]
//! fn SomeOtherComp() -> Element {
//!     rsx!(
//!         label {
//!             "{COUNT}" // We can use the global signal here again
//!         }
//!     )
//! }
//! ```
//!
//! #### You can now learn about [Lifecycle](crate::_docs::state_management::lifecycle).
