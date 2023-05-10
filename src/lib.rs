// Demo some lifetime questions around reqwest `Resolve` trait

use futures::future::FutureExt;
use reqwest::dns::Addrs;
use std::error::Error as StdError;
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use trust_dns_resolver::TokioAsyncResolver;

/// Suppose that we want to provide reqwest with a custom DNS resolver.  We can
/// do this by providing an object that impls its `Resolve` trait.  Here's an
/// example that uses a `trust_dns_resolver::TokioAsyncResolver` under the hood.
/// Here's how you might use it:
///
/// ```
/// // Create a reqwest client with a custom DNS resolver that always uses
/// // 1.1.1.1 (just as an example of wanting a custom resolver).
/// let raw_resolver = {
///     let dns_addr = "1.1.1.1:53".parse().unwrap();
///     let mut resolver_config = ResolverConfig::new();
///     resolver_config.add_name_server(NameServerConfig {
///         socket_addr: dns_addr,
///         protocol: Protocol::Udp,
///         tls_dns_name: None,
///         trust_nx_responses: false,
///         bind_addr: None,
///     });
///
///     TokioAsyncResolver::tokio(
///         resolver_config,
///         ResolverOpts::default()
///     ).unwrap();
/// };
///
/// let my_resolver = Arc::new(CustomDnsResolver::new(raw_resolver));
/// let _client =
///     reqwest::ClientBuilder::new().dns_resolver(my_resolver).build();
/// ```
pub struct CustomDnsResolver {
    // Note that we have to store an `Arc` here because the definition of the
    // `Resolve` trait seems to require that the returned Future outlive the
    // resolver itself?
    resolver: Arc<TokioAsyncResolver>,
}

impl CustomDnsResolver {
    pub fn new(resolver: TokioAsyncResolver) -> CustomDnsResolver {
        CustomDnsResolver { resolver: Arc::new(resolver) }
    }
}

impl reqwest::dns::Resolve for CustomDnsResolver {
    fn resolve(
        &self,
        name: hyper::client::connect::dns::Name,
    ) -> reqwest::dns::Resolving {
        // Compare to the impl of MyResolve below.  Here, we have to clone the
        // resolver and use an extra async block that we can move the Arc into.
        let resolver = self.resolver.clone();
        async move { do_resolve(&resolver, name).await }.boxed()
    }
}

// Here's a similar trait with different lifetime bounds that make it simpler
// for a use case like this.  The difference is just that the Future returned
// has the same lifetime as the Resolver itself.

/// Nearly the same as `Resolving`, but the Future has lifetime 'a.
pub type MyResolving<'a> = Pin<
    Box<
        dyn Future<Output = Result<Addrs, Box<dyn StdError + Send + Sync>>>
            + Send
            + 'a,
    >,
>;

/// Same as `Resolve`, but using `MyResolving<'a>` in place of `Resolving`.
pub trait MyResolve: Send + Sync {
    fn resolve<'a>(
        &'a self,
        name: hyper::client::connect::dns::Name,
    ) -> MyResolving<'a>;
}

/// This wrapper doesn't need an Arc.
pub struct MyCustomDnsResolver {
    resolver: TokioAsyncResolver,
}

impl MyCustomDnsResolver {
    pub fn new(resolver: TokioAsyncResolver) -> MyCustomDnsResolver {
        MyCustomDnsResolver { resolver }
    }
}

impl MyResolve for MyCustomDnsResolver {
    fn resolve(&self, name: hyper::client::connect::dns::Name) -> MyResolving {
        do_resolve(&self.resolver, name).boxed()
    }
}

// We'd like it to look like this, but it can't.
// impl reqwest::dns::Resolve for MyCustomDnsResolver {
//     fn resolve(
//         &self,
//         name: hyper::client::connect::dns::Name,
//     ) -> reqwest::dns::Resolving {
//         do_resolve(&self.resolver, name).boxed()
//     }
// }

async fn do_resolve(
    resolver: &TokioAsyncResolver,
    name: hyper::client::connect::dns::Name
) -> Result<Addrs, Box<dyn StdError + Send + Sync>> {
    let list = resolver.lookup_ip(name.as_str()).await?;
    Ok(Box::new(list.into_iter().map(|s| {
        // The port number is not used here.
        SocketAddr::from((s, 0))
    })) as Addrs)
}
