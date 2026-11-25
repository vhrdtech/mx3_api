//! What a URL names for the library: its host, the site the host belongs to, the target of a redirect page.

use url::Url;

/// A URL's host, lower case, without a trailing dot.
pub fn host(url: &str) -> Option<String> {
    Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.trim_end_matches('.').to_lowercase()))
}

/// The site a host belongs to: its last two labels, three below a two-letter country's `co`, `com`, `ne`...
/// (`ds.example.co.jp` -> `example.co.jp`, `www.example.com.tw` -> `example.com.tw`).
pub fn site(host: &str) -> String {
    let l: Vec<&str> = host.split('.').collect();
    let n = l.len();
    let country_second = n >= 3
        && l[n - 1].len() == 2
        && ["co", "com", "ne", "net", "or", "org", "ac", "gov", "edu"].contains(&l[n - 2]);
    let take = if country_second { 3 } else { 2 };
    l[n.saturating_sub(take)..].join(".")
}

/// Rounds of percent-decoding a redirect target may need (encoded once or twice).
const DECODE_ROUNDS_MAX: usize = 3;

/// A redirect page's target on the same site: `.../suppproductinfo.tsp?...&gotoUrl=<encoded URL>` -> that URL.
/// Anything else comes back unchanged.
pub fn direct(url: &str) -> String {
    let Ok(u) = Url::parse(url) else {
        return url.to_owned();
    };
    let Some(h) = u.host_str().map(str::to_lowercase) else {
        return url.to_owned();
    };
    let Some(mut to) = u
        .query_pairs()
        .find(|(k, _)| k.eq_ignore_ascii_case("gotoUrl"))
        .map(|(_, v)| v.into_owned())
    else {
        return url.to_owned();
    };
    for _ in 0..DECODE_ROUNDS_MAX {
        if to.contains("://") {
            break;
        }
        to = percent_decode(&to);
    }
    match Url::parse(&to) {
        Ok(mut t)
            if matches!(t.scheme(), "http" | "https")
                && t.host_str()
                    .is_some_and(|th| site(&th.to_lowercase()) == site(&h)) =>
        {
            // the redirect page says http://; the site serves https
            if t.scheme() == "http" && u.scheme() == "https" && t.set_scheme("https").is_err() {
                return url.to_owned();
            }
            t.to_string()
        }
        _ => url.to_owned(),
    }
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while let Some(&c) = b.get(i) {
        let hex = b
            .get(i + 1..i + 3)
            .and_then(|h| std::str::from_utf8(h).ok())
            .and_then(|h| u8::from_str_radix(h, 16).ok());
        match (c, hex) {
            (b'%', Some(v)) => {
                out.push(v);
                i += 3;
            }
            (b'+', _) => {
                out.push(b' ');
                i += 1;
            }
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sites_of_hosts() {
        assert_eq!(site("ds.example.co.jp"), "example.co.jp");
        assert_eq!(site("www.example.com.tw"), "example.com.tw");
        assert_eq!(site("www.example.com"), "example.com");
        assert_eq!(site("localhost"), "localhost");
        assert_eq!(
            host("https://WWW.Example.com./a"),
            Some("www.example.com".into())
        );
    }

    #[test]
    fn a_redirect_page_is_seen_through_on_its_own_site() {
        let once = "https://www.example.com/docs/go.tsp?distId=10&gotoUrl=https%3A%2F%2Fwww.example.com%2Flit%2Fgpn%2Fpart1";
        assert_eq!(direct(once), "https://www.example.com/lit/gpn/part1");
        let twice =
            "https://www.example.com/go?gotoUrl=http%253A%252F%252Fdocs.example.com%252Fp.pdf";
        assert_eq!(direct(twice), "https://docs.example.com/p.pdf");
        let elsewhere =
            "https://www.example.com/go?gotoUrl=https%3A%2F%2Fother.example.org%2Fp.pdf";
        assert_eq!(direct(elsewhere), elsewhere);
        assert_eq!(direct("https://a.example/p.pdf"), "https://a.example/p.pdf");
        assert_eq!(direct("not a url"), "not a url");
    }
}
