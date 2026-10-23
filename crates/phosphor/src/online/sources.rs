//! The libraries the Online panel searches: for each one, the search address, how to read its answer and how to find
//! the file to download. All of them are free and need no key. Parsing is kept apart from the network so tests can
//! feed it saved answers.

use serde_json::Value;

use super::{Item, Kind};
use crate::support::url_encode;

/// Results asked for per search page.
pub const PAGE: usize = 24;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Source {
    Nasa,
    Openverse,
    Commons,
    Archive,
    Artic,
}

impl Source {
    pub const ALL: [Source; 5] = [Source::Nasa, Source::Commons, Source::Archive, Source::Openverse, Source::Artic];

    pub fn name(self) -> &'static str {
        match self {
            Source::Nasa => "NASA",
            Source::Openverse => "Openverse",
            Source::Commons => "Wikimedia Commons",
            Source::Archive => "Internet Archive",
            Source::Artic => "Art Institute of Chicago",
        }
    }

    pub fn about(self) -> &'static str {
        match self {
            Source::Nasa => "NASA's pictures and videos of space, rockets and Earth. Public domain.",
            Source::Openverse => "Millions of Creative Commons photos from Flickr, museums and more. Some licences need a credit or forbid paid use.",
            Source::Commons => "Wikipedia's media library: photos and videos under open licences.",
            Source::Archive => "Old films from the Prelinger Archives (ads, educational and industrial films). Public domain.",
            Source::Artic => "Paintings and art from the Art Institute of Chicago's public-domain collection (CC0).",
        }
    }

    /// What it has.
    pub fn kinds(self) -> &'static [Kind] {
        match self {
            Source::Nasa | Source::Commons => &[Kind::Video, Kind::Picture],
            Source::Archive => &[Kind::Video],
            Source::Openverse | Source::Artic => &[Kind::Picture],
        }
    }

    /// The search address for one page (1 = first) of `kind`.
    pub fn search_url(self, query: &str, kind: Kind, page: usize, commercial: bool) -> String {
        let q = url_encode(query.trim());
        let page = page.max(1);
        match self {
            Source::Nasa => {
                let media = if kind == Kind::Video { "video" } else { "image" };
                format!("https://images-api.nasa.gov/search?q={q}&media_type={media}&page={page}&page_size={PAGE}")
            }
            Source::Openverse => {
                let only = if commercial { "&license_type=commercial" } else { "" };
                // Anonymous searches get at most 20 a page.
                format!("https://api.openverse.org/v1/images/?q={q}&page={page}&page_size=20&mature=false{only}")
            }
            Source::Commons => {
                let filetype = if kind == Kind::Video { "video" } else { "bitmap" };
                let search = url_encode(&format!("{} filetype:{filetype}", query.trim()));
                format!(
                    "https://commons.wikimedia.org/w/api.php?action=query&format=json&generator=search&gsrnamespace=6&gsrsearch={search}&gsrlimit={PAGE}&gsroffset={}&prop=imageinfo&iiprop=url|mime|size|extmetadata&iiurlwidth=320&iiextmetadatafilter=LicenseShortName|LicenseUrl|Artist|ObjectName",
                    (page - 1) * PAGE
                )
            }
            Source::Archive => {
                let search = url_encode(&format!("({}) AND mediatype:movies AND collection:prelinger", query.trim()));
                format!("https://archive.org/advancedsearch.php?q={search}&fl%5B%5D=identifier&fl%5B%5D=title&fl%5B%5D=creator&fl%5B%5D=year&rows={PAGE}&page={page}&output=json")
            }
            Source::Artic => format!(
                "https://api.artic.edu/api/v1/artworks/search?q={q}&query%5Bterm%5D%5Bis_public_domain%5D=true&fields=id,title,artist_title,image_id&limit={PAGE}&page={page}"
            ),
        }
    }

    /// The items in a search answer.
    pub fn parse(self, kind: Kind, body: &[u8]) -> Result<Vec<Item>, String> {
        let v: Value = serde_json::from_slice(body).map_err(|_| format!("{} sent something unreadable", self.name()))?;
        Ok(match self {
            Source::Nasa => nasa(&v, kind),
            Source::Openverse => openverse(&v),
            Source::Commons => commons(&v, kind),
            Source::Archive => archive(&v),
            Source::Artic => artic(&v),
        })
    }
}

fn text(v: &Value) -> String {
    v.as_str().unwrap_or("").trim().to_string()
}

/// Wikimedia's "Artist" is a bit of HTML: just its words.
fn strip_tags(html: &str) -> String {
    let mut out = String::new();
    let mut inside = false;
    for c in html.chars() {
        match c {
            '<' => inside = true,
            '>' => inside = false,
            _ if !inside => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn https(url: &str) -> String {
    url.strip_prefix("http://").map_or_else(|| url.to_string(), |rest| format!("https://{rest}"))
}

fn nasa(v: &Value, kind: Kind) -> Vec<Item> {
    let items = v["collection"]["items"].as_array().cloned().unwrap_or_default();
    items
        .iter()
        .filter_map(|it| {
            let d = &it["data"][0];
            let id = text(&d["nasa_id"]);
            let links = it["links"].as_array()?;
            let thumb = links.iter().find(|l| l["rel"] == "preview").or(links.first()).map(|l| https(&text(&l["href"])))?;
            let author = text(&d["photographer"]).replace("NASA or National Aeronautics and Space Administration", "NASA");
            Some(Item {
                source: Source::Nasa,
                kind,
                title: text(&d["title"]),
                thumb,
                file: None,
                list: https(&text(&it["href"])),
                size: None,
                credit: phosphor_engine::Credit {
                    title: text(&d["title"]),
                    author: if author.is_empty() { text(&d["center"]) } else { author },
                    licence: "Public domain".into(),
                    licence_url: "https://www.nasa.gov/nasa-brand-center/images-and-media/".into(),
                    page: format!("https://images.nasa.gov/details/{id}"),
                    source: "NASA".into(),
                },
                id,
            })
        })
        .collect()
}

fn openverse(v: &Value) -> Vec<Item> {
    let items = v["results"].as_array().cloned().unwrap_or_default();
    items
        .iter()
        .filter_map(|r| {
            let file = text(&r["url"]);
            (!file.is_empty()).then(|| {
                let version = text(&r["license_version"]);
                let code = text(&r["license"]).to_uppercase();
                let licence = match code.as_str() {
                    "CC0" => "CC0".to_string(),
                    "PDM" => "Public domain".to_string(),
                    _ => format!("CC {code} {version}").trim().to_string(),
                };
                let provider = text(&r["provider"]);
                Item {
                    source: Source::Openverse,
                    kind: Kind::Picture,
                    id: text(&r["id"]),
                    title: text(&r["title"]),
                    thumb: text(&r["thumbnail"]),
                    file: Some(file),
                    list: String::new(),
                    size: r["filesize"].as_u64(),
                    credit: phosphor_engine::Credit {
                        title: text(&r["title"]),
                        author: text(&r["creator"]),
                        licence,
                        licence_url: text(&r["license_url"]),
                        page: text(&r["foreign_landing_url"]),
                        source: if provider.is_empty() { "Openverse".into() } else { format!("Openverse ({provider})") },
                    },
                }
            })
        })
        .collect()
}

fn commons(v: &Value, kind: Kind) -> Vec<Item> {
    let Some(pages) = v["query"]["pages"].as_object() else { return Vec::new() };
    let mut pages: Vec<&Value> = pages.values().collect();
    // The search's own order.
    pages.sort_by_key(|p| p["index"].as_u64().unwrap_or(u64::MAX));
    pages
        .into_iter()
        .filter_map(|p| {
            let info = &p["imageinfo"][0];
            let meta = &info["extmetadata"];
            let file = text(&info["url"]);
            let mime = text(&info["mime"]);
            // Only what plays: no PDFs, sound files or SVGs posing as pictures.
            let wanted = if kind == Kind::Video { mime.starts_with("video/") } else { mime.starts_with("image/") && mime != "image/svg+xml" };
            if file.is_empty() || !wanted {
                return None;
            }
            let title = text(&meta["ObjectName"]["value"]);
            let title = if title.is_empty() { text(&p["title"]).trim_start_matches("File:").to_string() } else { title };
            let thumb = text(&info["thumburl"]);
            // Photos far bigger than a screen (Commons has 100-megapixel ones): Wikimedia's 3840-wide copy instead.
            let big = kind == Kind::Picture && info["width"].as_u64().is_some_and(|w| w > BIGGEST);
            let (file, size) = match big.then(|| scaled(&thumb, BIGGEST)).flatten() {
                Some(copy) => (copy, None),
                None => (file.split('?').next().unwrap_or(&file).to_string(), info["size"].as_u64()),
            };
            Some(Item {
                source: Source::Commons,
                kind,
                id: p["pageid"].to_string(),
                title: title.clone(),
                thumb,
                file: Some(file),
                list: String::new(),
                size,
                credit: phosphor_engine::Credit {
                    title,
                    author: strip_tags(&text(&meta["Artist"]["value"])),
                    licence: text(&meta["LicenseShortName"]["value"]),
                    licence_url: text(&meta["LicenseUrl"]["value"]),
                    page: text(&info["descriptionurl"]),
                    source: "Wikimedia Commons".into(),
                },
            })
        })
        .collect()
}

/// Pictures wider than this come as Wikimedia's scaled copy.
const BIGGEST: u64 = 3840;

/// A Wikimedia thumbnail address (`…/330px-Name.jpg?…`) at another width.
fn scaled(thumb: &str, width: u64) -> Option<String> {
    let thumb = thumb.split('?').next()?;
    let (dir, last) = thumb.rsplit_once('/')?;
    let (px, name) = last.split_once("px-")?;
    px.chars().all(|c| c.is_ascii_digit()).then(|| format!("{dir}/{width}px-{name}"))
}

fn archive(v: &Value) -> Vec<Item> {
    let docs = v["response"]["docs"].as_array().cloned().unwrap_or_default();
    docs.iter()
        .filter_map(|d| {
            let id = text(&d["identifier"]);
            (!id.is_empty()).then(|| {
                // Creator can be a string or a list.
                let author = match &d["creator"] {
                    Value::Array(a) => a.iter().map(text).collect::<Vec<_>>().join(", "),
                    c => text(c),
                };
                Item {
                    source: Source::Archive,
                    kind: Kind::Video,
                    title: text(&d["title"]),
                    thumb: format!("https://archive.org/services/img/{id}"),
                    file: None,
                    list: format!("https://archive.org/metadata/{id}"),
                    size: None,
                    credit: phosphor_engine::Credit {
                        title: text(&d["title"]),
                        author,
                        licence: "Public domain".into(),
                        licence_url: "https://archive.org/details/prelinger".into(),
                        page: format!("https://archive.org/details/{id}"),
                        source: "Internet Archive (Prelinger Archives)".into(),
                    },
                    id,
                }
            })
        })
        .collect()
}

fn artic(v: &Value) -> Vec<Item> {
    let data = v["data"].as_array().cloned().unwrap_or_default();
    data.iter()
        .filter_map(|d| {
            let image = text(&d["image_id"]);
            if image.is_empty() {
                return None;
            }
            let id = d["id"].to_string();
            let iiif = format!("https://www.artic.edu/iiif/2/{image}/full");
            Some(Item {
                source: Source::Artic,
                kind: Kind::Picture,
                title: text(&d["title"]),
                thumb: format!("{iiif}/200,/0/default.jpg"),
                file: Some(format!("{iiif}/1686,/0/default.jpg")),
                list: String::new(),
                size: None,
                credit: phosphor_engine::Credit {
                    title: text(&d["title"]),
                    author: text(&d["artist_title"]),
                    licence: "CC0".into(),
                    licence_url: "https://creativecommons.org/publicdomain/zero/1.0/".into(),
                    page: format!("https://www.artic.edu/artworks/{id}"),
                    source: "Art Institute of Chicago".into(),
                },
                id,
            })
        })
        .collect()
}

/// For sources that list an item's files separately (NASA, Internet Archive): the file to download from that list,
/// and its size when known.
pub fn pick_file(item: &Item, list: &[u8]) -> Result<(String, Option<u64>), String> {
    let v: Value = serde_json::from_slice(list).map_err(|_| "couldn't read the list of files".to_string())?;
    match item.source {
        Source::Nasa => {
            let urls: Vec<String> = v.as_array().into_iter().flatten().map(|u| https(&text(u))).collect();
            // Big enough for a show, not a master file: large, then the original, then smaller ones.
            let order: &[&str] = if item.kind == Kind::Video {
                &["~large.mp4", "~orig.mp4", "~medium.mp4", "~mobile.mp4", ".mp4", ".mov"]
            } else {
                &["~large.jpg", "~orig.jpg", "~medium.jpg", "~orig.png", ".jpg", ".png"]
            };
            order.iter().find_map(|end| urls.iter().find(|u| u.to_ascii_lowercase().ends_with(end))).map(|u| (u.clone(), None)).ok_or_else(|| "NASA has no file to play for this one".into())
        }
        Source::Archive => {
            let id = text(&v["metadata"]["identifier"]);
            let id = if id.is_empty() { item.id.clone() } else { id };
            let files: Vec<(String, u64)> = v["files"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|f| {
                    let name = text(&f["name"]);
                    let size = f["size"].as_str().and_then(|s| s.parse().ok()).or(f["size"].as_u64()).unwrap_or(0);
                    name.to_ascii_lowercase().ends_with(".mp4").then_some((name, size))
                })
                .collect();
            // The biggest MP4 under 120 MB (old films are low resolution anyway; the originals can be huge), else the
            // smallest.
            const CAP: u64 = 120 << 20;
            let best = files.iter().filter(|(_, s)| *s <= CAP).max_by_key(|(_, s)| *s).or_else(|| files.iter().min_by_key(|(_, s)| *s));
            best.map(|(name, size)| {
                let path: Vec<String> = name.split('/').map(url_encode).collect();
                (format!("https://archive.org/download/{}/{}", url_encode(&id), path.join("/")), (*size > 0).then_some(*size))
            })
            .ok_or_else(|| "The Internet Archive has no MP4 of this film".into())
        }
        _ => item.file.clone().map(|f| (f, item.size)).ok_or_else(|| "no file".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Saved (shortened) answers from each library read into items with a picture, a file and a credit.
    #[test]
    fn every_library_answer_is_read() {
        let nasa_answer = br#"{"collection":{"items":[{"href":"http://images-assets.nasa.gov/video/x1/collection.json","data":[{"center":"KSC","media_type":"video","nasa_id":"x1","photographer":"NASA or National Aeronautics and Space Administration","title":"Trifid"}],"links":[{"href":"https://images-assets.nasa.gov/video/x1/x1~small.jpg","rel":"alternate"},{"href":"https://images-assets.nasa.gov/video/x1/x1~thumb.jpg","rel":"preview"}]}]}}"#;
        let n = Source::Nasa.parse(Kind::Video, nasa_answer).unwrap();
        assert_eq!(n.len(), 1);
        assert_eq!(n[0].thumb, "https://images-assets.nasa.gov/video/x1/x1~thumb.jpg");
        assert_eq!(n[0].list, "https://images-assets.nasa.gov/video/x1/collection.json", "always https");
        assert_eq!((n[0].credit.author.as_str(), n[0].credit.licence.as_str()), ("NASA", "Public domain"));
        let files = br#"["http://images-assets.nasa.gov/video/x1/x1~orig.mp4","http://images-assets.nasa.gov/video/x1/x1~mobile.mp4","http://images-assets.nasa.gov/video/x1/x1~thumb.jpg"]"#;
        assert_eq!(pick_file(&n[0], files).unwrap().0, "https://images-assets.nasa.gov/video/x1/x1~orig.mp4");

        let ov = br#"{"results":[{"id":"a","title":"neon","url":"https://live.staticflickr.com/1/a_b.jpg","creator":"ranjit","license":"by-nc","license_version":"2.0","license_url":"https://creativecommons.org/licenses/by-nc/2.0/","provider":"flickr","foreign_landing_url":"https://www.flickr.com/photos/x/1","thumbnail":"https://api.openverse.org/v1/images/a/thumb/"},{"id":"b","title":"no file"}]}"#;
        let o = Source::Openverse.parse(Kind::Picture, ov).unwrap();
        assert_eq!(o.len(), 1, "items without a file are skipped");
        assert_eq!(o[0].credit.licence, "CC BY-NC 2.0");
        assert!(!o[0].credit.commercial());
        assert_eq!(o[0].credit.line(), "\"neon\" by ranjit (CC BY-NC 2.0), Openverse (flickr): https://www.flickr.com/photos/x/1");

        let cm = br#"{"query":{"pages":{"2":{"pageid":2,"index":2,"title":"File:B.pdf","imageinfo":[{"url":"https://upload.wikimedia.org/b.pdf","mime":"application/pdf"}]},"1":{"pageid":1,"index":1,"title":"File:Aurora.webm","imageinfo":[{"size":99692793,"thumburl":"https://thumb.wikimedia.org/a.jpg","url":"https://upload.wikimedia.org/a.webm?utm_source=x","descriptionurl":"https://commons.wikimedia.org/wiki/File:Aurora.webm","mime":"video/webm","extmetadata":{"Artist":{"value":"<a href=\"//x\">Giles Laurent</a>"},"LicenseShortName":{"value":"CC BY-SA 4.0"}}}]}}}}"#;
        let c = Source::Commons.parse(Kind::Video, cm).unwrap();
        assert_eq!(c.len(), 1, "the PDF is left out");
        assert_eq!(c[0].file.as_deref(), Some("https://upload.wikimedia.org/a.webm"));
        assert_eq!((c[0].title.as_str(), c[0].credit.author.as_str(), c[0].size), ("Aurora.webm", "Giles Laurent", Some(99692793)));
        assert!(c[0].credit.commercial());
        let huge = br#"{"query":{"pages":{"1":{"pageid":1,"index":1,"title":"File:N.jpg","imageinfo":[{"size":35465830,"width":6837,"thumburl":"https://thumb.wikimedia.org/wikipedia/commons/thumb/9/99/N.jpg/330px-N.jpg?utm_source=x","url":"https://upload.wikimedia.org/wikipedia/commons/9/99/N.jpg","mime":"image/jpeg"}]}}}}"#;
        let h = Source::Commons.parse(Kind::Picture, huge).unwrap();
        assert_eq!(h[0].file.as_deref(), Some("https://thumb.wikimedia.org/wikipedia/commons/thumb/9/99/N.jpg/3840px-N.jpg"), "a screen-sized copy");
        assert_eq!(h[0].size, None);

        let ar = br#"{"response":{"docs":[{"identifier":"its_no_picnic","title":"It's No Picnic","creator":["A","B"]}]}}"#;
        let a = Source::Archive.parse(Kind::Video, ar).unwrap();
        assert_eq!((a[0].thumb.as_str(), a[0].credit.author.as_str()), ("https://archive.org/services/img/its_no_picnic", "A, B"));
        let meta = br#"{"metadata":{"identifier":"its_no_picnic"},"files":[{"name":"its_no_picnic.mp4","size":"642265081"},{"name":"its_no_picnic_512kb.mp4","size":"91000000"},{"name":"its_no_picnic.ogv","size":"117817681"}]}"#;
        assert_eq!(pick_file(&a[0], meta).unwrap(), ("https://archive.org/download/its_no_picnic/its_no_picnic_512kb.mp4".into(), Some(91000000)), "the original is too big");

        let ai = br#"{"data":[{"id":282,"title":"Waves","artist_title":"Hokusai","image_id":"99af"},{"id":3,"title":"no picture","image_id":null}]}"#;
        let w = Source::Artic.parse(Kind::Picture, ai).unwrap();
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].file.as_deref(), Some("https://www.artic.edu/iiif/2/99af/full/1686,/0/default.jpg"));
        assert_eq!(w[0].credit.page, "https://www.artic.edu/artworks/282");
    }

    #[test]
    fn search_addresses_encode_the_words_and_page() {
        let u = Source::Commons.search_url("northern lights", Kind::Video, 2, false);
        assert!(u.contains("gsrsearch=northern%20lights%20filetype%3Avideo") && u.contains(&format!("gsroffset={PAGE}")), "{u}");
        assert!(Source::Openverse.search_url("a&b", Kind::Picture, 1, true).contains("q=a%26b&") && Source::Openverse.search_url("x", Kind::Picture, 1, true).contains("license_type=commercial"));
        assert!(Source::Nasa.search_url("moon", Kind::Picture, 3, false).contains("media_type=image&page=3"));
        assert!(Source::Archive.search_url("fire", Kind::Video, 1, false).contains("collection%3Aprelinger"));
        for s in Source::ALL {
            assert!(!s.kinds().is_empty() && !s.about().is_empty());
        }
    }

    #[test]
    fn non_commercial_licences_are_spotted() {
        use phosphor_engine::licence_commercial as ok;
        assert!(ok("CC BY 4.0") && ok("CC0") && ok("Public domain") && ok("CC BY-SA 3.0"));
        assert!(!ok("CC BY-NC 2.0") && !ok("CC BY-NC-SA 4.0") && !ok("cc by-nc-nd"));
    }
}
