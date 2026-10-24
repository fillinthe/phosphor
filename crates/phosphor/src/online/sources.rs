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
    Iconify,
    PolyHaven,
    MilkDrop,
    GoogleFonts,
    IsfOnline,
    /// An X account's GIFs (`super::x`, through gallery-dl; the "query" is the account).
    X,
}

impl Source {
    pub const ALL: [Source; 11] = [
        Source::Nasa,
        Source::Commons,
        Source::Archive,
        Source::Openverse,
        Source::Artic,
        Source::Iconify,
        Source::PolyHaven,
        Source::MilkDrop,
        Source::GoogleFonts,
        Source::IsfOnline,
        Source::X,
    ];

    /// The search answer is the library's whole list, the same every time (kept for the session, `super::get_cached`).
    pub fn whole_list(self) -> bool {
        matches!(self, Source::PolyHaven | Source::MilkDrop | Source::GoogleFonts)
    }

    pub fn name(self) -> &'static str {
        match self {
            Source::Nasa => "NASA",
            Source::Openverse => "Openverse",
            Source::Commons => "Wikimedia Commons",
            Source::Archive => "Internet Archive",
            Source::Artic => "Art Institute of Chicago",
            Source::Iconify => "Iconify",
            Source::PolyHaven => "Poly Haven",
            Source::MilkDrop => "MilkDrop presets",
            Source::GoogleFonts => "Google Fonts",
            Source::IsfOnline => "ISF shaders",
            Source::X => "X (Twitter)",
        }
    }

    pub fn about(self) -> &'static str {
        match self {
            Source::Nasa => "NASA's pictures and videos of space, rockets and Earth. Public domain.",
            Source::Openverse => "Millions of Creative Commons photos from Flickr, museums and more. Some licences need a credit or forbid paid use.",
            Source::Commons => "Wikipedia's media library: photos and videos under open licences.",
            Source::Archive => "Old public-domain films from the Prelinger Archives, and Creative Commons music from netlabels.",
            Source::Artic => "Paintings and art from the Art Institute of Chicago's public-domain collection (CC0).",
            Source::Iconify => "Over 200,000 icons and symbols from open icon sets, as sharp SVG pictures (white, to colour with effects).",
            Source::PolyHaven => "Photo textures (stone, wood, metal, fabric…) for 3D models and backgrounds. CC0.",
            Source::X => "An X account's animated GIFs (needs gallery-dl and your browser's X sign-in).",
            Source::IsfOnline => "Thousands of effects and generators people shared on isf.video (licences vary: check the credit).",
            Source::GoogleFonts => "About 1,900 open-licence font families for text layers.",
            Source::MilkDrop => "Nearly 10,000 MilkDrop visualisers (projectM's \"Cream of the Crop\" pack, sorted by Isosceles) that move to the music.",
        }
    }

    /// What it has.
    pub fn kinds(self) -> &'static [Kind] {
        match self {
            Source::Nasa | Source::Commons => &[Kind::Video, Kind::Picture],
            Source::Archive => &[Kind::Video, Kind::Music],
            Source::Openverse | Source::Artic => &[Kind::Picture],
            Source::Iconify => &[Kind::Icon],
            Source::PolyHaven => &[Kind::Texture],
            Source::MilkDrop => &[Kind::Visualizer],
            Source::GoogleFonts => &[Kind::Font],
            Source::IsfOnline => &[Kind::Shader],
            Source::X => &[Kind::Gif],
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
                // Films: the Prelinger Archives (public domain); music: netlabel releases (Creative Commons).
                // (Music: only releases with a Creative Commons licence and MP3s to play.)
                let only = if kind == Kind::Music {
                    "mediatype:audio AND collection:netlabels AND licenseurl:*creativecommons* AND format:\"VBR MP3\""
                } else {
                    "mediatype:movies AND collection:prelinger"
                };
                let licence = if kind == Kind::Music && commercial { " AND NOT licenseurl:*nc*" } else { "" };
                let search = url_encode(&format!("({}) AND {only}{licence}", query.trim()));
                format!("https://archive.org/advancedsearch.php?q={search}&fl%5B%5D=identifier&fl%5B%5D=title&fl%5B%5D=creator&fl%5B%5D=licenseurl&rows={PAGE}&page={page}&output=json")
            }
            // Iconify pages hold at least 32.
            Source::Iconify => format!("https://api.iconify.design/search?query={q}&limit=32&start={}", (page - 1) * 32),
            // Poly Haven has no search: the whole list (filtered here, `poly_haven`).
            Source::PolyHaven => "https://api.polyhaven.com/assets?type=textures".to_string(),
            Source::MilkDrop => format!("https://api.github.com/repos/{MILK_REPO}/git/trees/master?recursive=1"),
            // Not an address: gallery-dl lists the account (`super::x::list`).
            Source::X => format!("x:{}", query.trim()),
            // The ISF editor's own search (18 a page).
            Source::IsfOnline => format!("https://editor.isf.video/api/search?q={q}&page={}&from={}", page - 1, (page - 1) * ISF_PAGE),
            // Every family (the list behind fonts.google.com; no key), searched here.
            Source::GoogleFonts => "https://fonts.google.com/metadata/fonts".to_string(),
            Source::Artic => format!(
                "https://api.artic.edu/api/v1/artworks/search?q={q}&query%5Bterm%5D%5Bis_public_domain%5D=true&fields=id,title,artist_title,image_id&limit={PAGE}&page={page}"
            ),
        }
    }

    /// The items in a search answer (for a library without its own search: the matches for `query` on `page`).
    pub fn parse_page(self, kind: Kind, query: &str, page: usize, body: &[u8]) -> Result<Vec<Item>, String> {
        if self.whole_list() {
            let v: Value = serde_json::from_slice(unguard(body)).map_err(|_| format!("{} sent something unreadable", self.name()))?;
            return Ok(match self {
                Source::PolyHaven => poly_haven(&v, query, page),
                Source::GoogleFonts => google_fonts(&v, query, page),
                _ => milkdrop(&v, query, page),
            });
        }
        self.parse(kind, body)
    }

    /// The items in a search answer.
    pub fn parse(self, kind: Kind, body: &[u8]) -> Result<Vec<Item>, String> {
        let v: Value = serde_json::from_slice(unguard(body)).map_err(|_| format!("{} sent something unreadable", self.name()))?;
        Ok(match self {
            Source::Nasa => nasa(&v, kind),
            Source::Openverse => openverse(&v),
            Source::Commons => commons(&v, kind),
            Source::Archive => archive(&v, kind),
            Source::Artic => artic(&v),
            Source::Iconify => iconify(&v),
            Source::PolyHaven => poly_haven(&v, "", 1),
            Source::MilkDrop => milkdrop(&v, "", 1),
            Source::GoogleFonts => google_fonts(&v, "", 1),
            Source::IsfOnline => isf_online(&v),
            Source::X => Vec::new(),
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

/// A Creative Commons address as a licence name: ".../licenses/by-nc-sa/3.0/" → "CC BY-NC-SA 3.0".
pub fn cc_name(url: &str) -> String {
    let parts: Vec<&str> = url.split('/').filter(|p| !p.is_empty()).collect();
    if let Some(i) = parts.iter().position(|p| *p == "licenses") {
        let code = parts.get(i + 1).map_or(String::new(), |c| c.to_uppercase());
        let version = parts.get(i + 2).copied().unwrap_or("");
        return format!("CC {code} {version}").trim().to_string();
    }
    if url.contains("publicdomain/zero") {
        return "CC0".into();
    }
    if url.contains("publicdomain") {
        return "Public domain".into();
    }
    if url.is_empty() { "See its page".into() } else { url.to_string() }
}

fn archive(v: &Value, kind: Kind) -> Vec<Item> {
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
                let music = kind == Kind::Music;
                let licence_url = if music { text(&d["licenseurl"]) } else { "https://archive.org/details/prelinger".into() };
                Item {
                    source: Source::Archive,
                    kind,
                    title: text(&d["title"]),
                    thumb: format!("https://archive.org/services/img/{id}"),
                    file: None,
                    list: format!("https://archive.org/metadata/{id}"),
                    size: None,
                    credit: phosphor_engine::Credit {
                        title: text(&d["title"]),
                        author,
                        licence: if music { cc_name(&licence_url) } else { "Public domain".into() },
                        licence_url,
                        page: format!("https://archive.org/details/{id}"),
                        source: if music { "Internet Archive (netlabels)".into() } else { "Internet Archive (Prelinger Archives)".into() },
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

fn iconify(v: &Value) -> Vec<Item> {
    let sets = &v["collections"];
    v["icons"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|i| {
            let id = text(i);
            let (prefix, name) = id.split_once(':')?;
            let set = &sets[prefix];
            let svg = format!("https://api.iconify.design/{prefix}/{name}.svg?color=white");
            Some(Item {
                source: Source::Iconify,
                kind: Kind::Icon,
                title: format!("{} ({})", name.replace('-', " "), text(&set["name"])),
                thumb: format!("{svg}&height=192"),
                file: Some(format!("{svg}&height=1024")),
                list: String::new(),
                size: None,
                credit: phosphor_engine::Credit {
                    title: name.replace('-', " "),
                    author: text(&set["author"]["name"]),
                    licence: text(&set["license"]["title"]),
                    licence_url: text(&set["license"]["url"]),
                    page: format!("https://icon-sets.iconify.design/{prefix}/{name}/"),
                    source: format!("Iconify ({})", text(&set["name"])),
                },
                id: id.clone(),
            })
        })
        .collect()
}

/// Poly Haven's texture list, matched against every word of `query` (name, tags, categories), most downloaded first.
fn poly_haven(v: &Value, query: &str, page: usize) -> Vec<Item> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    let Some(all) = v.as_object() else { return Vec::new() };
    let mut found: Vec<(&String, &Value)> = all
        .iter()
        .filter(|(id, a)| {
            let mut hay = format!("{id} {}", text(&a["name"])).to_lowercase();
            for list in ["tags", "categories"] {
                for t in a[list].as_array().into_iter().flatten() {
                    hay.push(' ');
                    hay.push_str(&text(t).to_lowercase());
                }
            }
            words.iter().all(|w| hay.contains(w.as_str()))
        })
        .collect();
    found.sort_by_key(|(_, a)| std::cmp::Reverse(a["download_count"].as_u64().unwrap_or(0)));
    found
        .into_iter()
        .skip((page.max(1) - 1) * PAGE)
        .take(PAGE)
        .map(|(id, a)| {
            let author = a["authors"].as_object().map(|m| m.keys().cloned().collect::<Vec<_>>().join(", ")).unwrap_or_default();
            Item {
                source: Source::PolyHaven,
                kind: Kind::Texture,
                id: id.clone(),
                title: text(&a["name"]),
                thumb: format!("https://cdn.polyhaven.com/asset_img/thumbs/{id}.png?width=256&height=256"),
                file: None,
                list: format!("https://api.polyhaven.com/files/{id}"),
                size: None,
                credit: phosphor_engine::Credit {
                    title: text(&a["name"]),
                    author,
                    licence: "CC0".into(),
                    licence_url: "https://creativecommons.org/publicdomain/zero/1.0/".into(),
                    page: format!("https://polyhaven.com/a/{id}"),
                    source: "Poly Haven".into(),
                },
            }
        })
        .collect()
}

/// Shaders per page of the ISF editor's search.
const ISF_PAGE: usize = 18;

/// isf.video's shaders (public ones). Their licences aren't stated, so they count as not for paid work.
fn isf_online(v: &Value) -> Vec<Item> {
    v["shaders"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|s| s["private"] != true)
        .filter_map(|s| {
            let id = text(&s["id"]);
            let title = text(&s["title"]).trim_end_matches(".fs").to_string();
            (!id.is_empty() && !title.is_empty()).then(|| {
                let thumb = text(&s["thumbnailCloudinaryId"]);
                Item {
                    source: Source::IsfOnline,
                    kind: Kind::Shader,
                    title: title.clone(),
                    thumb: if thumb.is_empty() { String::new() } else { format!("https://res.cloudinary.com/hrlz5rsqo/image/upload/w_256/{thumb}.jpg") },
                    file: None,
                    list: format!("https://editor.isf.video/api/shaders/{id}"),
                    size: None,
                    credit: phosphor_engine::Credit {
                        title,
                        author: text(&s["username"]),
                        licence: "Unknown (shared on isf.video)".into(),
                        licence_url: String::new(),
                        page: format!("https://editor.isf.video/shaders/{id}"),
                        source: "isf.video".into(),
                    },
                    id,
                }
            })
        })
        .collect()
}

/// Google's answers can start with `)]}'` (a guard against being run as a script).
fn unguard(body: &[u8]) -> &[u8] {
    let start = body.iter().position(|b| *b == b'{' || *b == b'[').unwrap_or(0);
    &body[start..]
}

/// Font families whose name or category holds every word of `query`, most popular first.
fn google_fonts(v: &Value, query: &str, page: usize) -> Vec<Item> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    let mut found: Vec<&Value> = v["familyMetadataList"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|f| {
            let hay = format!("{} {}", text(&f["family"]), text(&f["category"])).to_lowercase();
            words.iter().all(|w| hay.contains(w.as_str()))
        })
        .collect();
    found.sort_by_key(|f| f["popularity"].as_u64().unwrap_or(u64::MAX));
    found
        .into_iter()
        .skip((page.max(1) - 1) * PAGE)
        .take(PAGE)
        .map(|f| {
            let family = text(&f["family"]);
            let designers = f["designers"].as_array().map(|d| d.iter().map(text).collect::<Vec<_>>().join(", ")).unwrap_or_default();
            Item {
                source: Source::GoogleFonts,
                kind: Kind::Font,
                id: family.clone(),
                title: format!("{family} ({})", text(&f["category"]).to_lowercase()),
                thumb: String::new(),
                file: None,
                list: format!("https://fonts.google.com/download/list?family={}", url_encode(&family)),
                size: None,
                credit: phosphor_engine::Credit {
                    title: family.clone(),
                    author: designers,
                    licence: "Open font licence (OFL, Apache or Ubuntu)".into(),
                    licence_url: "https://fonts.google.com/attribution".into(),
                    page: format!("https://fonts.google.com/specimen/{}", family.replace(' ', "+")),
                    source: "Google Fonts".into(),
                },
            }
        })
        .collect()
}

/// A font family's files to download (file name, address): the regular and bold styles when it has them (else its
/// first font, often one variable font holding every weight), at most four.
pub fn font_files(list: &[u8]) -> Result<Vec<(String, String)>, String> {
    let v: Value = serde_json::from_slice(unguard(list)).map_err(|_| "couldn't read the font's file list".to_string())?;
    let all: Vec<(String, String)> = v["manifest"]["fileRefs"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|f| (text(&f["filename"]), text(&f["url"])))
        .filter(|(n, u)| (n.ends_with(".ttf") || n.ends_with(".otf")) && u.starts_with("https://"))
        .map(|(n, u)| (n.rsplit('/').next().unwrap_or(&n).to_string(), u))
        .collect();
    let styles: Vec<(String, String)> = ["-Regular.", "-Bold.", "-Italic.", "-Black."].iter().filter_map(|s| all.iter().find(|(n, _)| n.contains(s)).cloned()).collect();
    let pick = if styles.is_empty() { all.into_iter().take(1).collect::<Vec<_>>() } else { styles };
    if pick.is_empty() {
        return Err("Google Fonts listed no font file for this family".into());
    }
    Ok(pick.into_iter().take(4).collect())
}

/// The MilkDrop preset pack on GitHub.
const MILK_REPO: &str = "projectM-visualizer/presets-cream-of-the-crop";

/// The pack's presets whose folder or name holds every word of `query` ("Dancer/Aurora/Royal - Mashup.milk").
fn milkdrop(v: &Value, query: &str, page: usize) -> Vec<Item> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    v["tree"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|f| text(&f["path"]))
        .filter(|p| p.to_ascii_lowercase().ends_with(".milk"))
        .filter(|p| {
            let hay = p.to_lowercase();
            words.iter().all(|w| hay.contains(w.as_str()))
        })
        .skip((page.max(1) - 1) * PAGE)
        .take(PAGE)
        .map(|path| {
            let (folder, file) = path.rsplit_once('/').unwrap_or(("", &path));
            let name = file.trim_end_matches(".milk").trim_end_matches(".MILK").to_string();
            // Preset names are mostly "Author - Title".
            let author = name.split_once(" - ").map(|(a, _)| a.trim().to_string()).unwrap_or_default();
            let encoded: Vec<String> = path.split('/').map(url_encode).collect();
            Item {
                source: Source::MilkDrop,
                kind: Kind::Visualizer,
                title: if folder.is_empty() { name.clone() } else { format!("{name} ({})", folder.replace('/', " › ")) },
                thumb: String::new(),
                file: Some(format!("https://raw.githubusercontent.com/{MILK_REPO}/master/{}", encoded.join("/"))),
                list: String::new(),
                size: None,
                credit: phosphor_engine::Credit {
                    title: name,
                    author,
                    licence: "Public domain (freely shared preset)".into(),
                    licence_url: format!("https://github.com/{MILK_REPO}/blob/master/LICENSE.md"),
                    page: format!("https://github.com/{MILK_REPO}/blob/master/{}", encoded.join("/")),
                    source: "projectM Cream of the Crop (Isosceles)".into(),
                },
                id: path,
            }
        })
        .collect()
}

/// For sources that list an item's files separately (NASA, Internet Archive): the file to download from that list,
/// and its size when known.
/// Smaller copies to fall back on when `pick_file`'s is too big (NASA's medium and mobile versions), biggest first.
pub fn smaller_files(item: &Item, list: &[u8]) -> Vec<String> {
    if item.source != Source::Nasa {
        return Vec::new();
    }
    let Ok(v) = serde_json::from_slice::<Value>(list) else { return Vec::new() };
    let urls: Vec<String> = v.as_array().into_iter().flatten().map(|u| https(&text(u))).collect();
    let order: &[&str] = if item.kind == Kind::Video { &["~medium.mp4", "~mobile.mp4"] } else { &["~medium.jpg", "~small.jpg"] };
    order.iter().filter_map(|end| urls.iter().find(|u| u.to_ascii_lowercase().ends_with(end)).cloned()).collect()
}

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
        Source::Archive if item.kind == Kind::Music => {
            // A release's first track (as MP3, the format every copy has).
            let id = text(&v["metadata"]["identifier"]);
            let id = if id.is_empty() { item.id.clone() } else { id };
            let mut tracks: Vec<(String, Option<u64>)> = v["files"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|f| text(&f["name"]).to_ascii_lowercase().ends_with(".mp3"))
                .map(|f| (text(&f["name"]), f["size"].as_str().and_then(|s| s.parse().ok())))
                .collect();
            tracks.sort();
            tracks
                .into_iter()
                .next()
                .map(|(name, size)| {
                    let path: Vec<String> = name.split('/').map(url_encode).collect();
                    (format!("https://archive.org/download/{}/{}", url_encode(&id), path.join("/")), size)
                })
                .ok_or_else(|| "This release has no MP3".into())
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
        Source::PolyHaven => {
            // The colour picture at 2K (a few MB; 4K and up are for close-ups).
            let diffuse = &v["Diffuse"];
            ["2k", "1k", "4k"]
                .iter()
                .find_map(|res| {
                    let f = &diffuse[*res]["jpg"];
                    let url = text(&f["url"]);
                    (!url.is_empty()).then(|| (url, f["size"].as_u64()))
                })
                .ok_or_else(|| "Poly Haven has no colour picture for this texture".into())
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
        assert_eq!(smaller_files(&n[0], files), ["https://images-assets.nasa.gov/video/x1/x1~mobile.mp4"], "the fallback");

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

        let music = br#"{"response":{"docs":[{"identifier":"bump006","title":"getting warmer EP","creator":"tatsu","licenseurl":"http://creativecommons.org/licenses/by-nc-sa/2.5/"}]}}"#;
        let m = Source::Archive.parse(Kind::Music, music).unwrap();
        assert_eq!((m[0].kind, m[0].credit.licence.as_str()), (Kind::Music, "CC BY-NC-SA 2.5"));
        assert!(!m[0].credit.commercial());
        let tracks = br#"{"metadata":{"identifier":"bump006"},"files":[{"name":"02-b.mp3","size":"1002661"},{"name":"cover.jpg"},{"name":"01-a song.mp3","size":"524911"}]}"#;
        assert_eq!(pick_file(&m[0], tracks).unwrap(), ("https://archive.org/download/bump006/01-a%20song.mp3".into(), Some(524911)), "the first track");
        assert_eq!(cc_name("https://creativecommons.org/publicdomain/zero/1.0/"), "CC0");
        assert!(Source::Archive.search_url("techno", Kind::Music, 1, true).contains("NOT%20licenseurl"), "paid work: no NC music");
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

        let ic = br#"{"icons":["ph:star-fill","bad"],"collections":{"ph":{"name":"Phosphor","author":{"name":"Phosphor Icons"},"license":{"title":"MIT","url":"https://x/LICENSE"}}}}"#;
        let i = Source::Iconify.parse(Kind::Icon, ic).unwrap();
        assert_eq!(i.len(), 1);
        assert_eq!(i[0].file.as_deref(), Some("https://api.iconify.design/ph/star-fill.svg?color=white&height=1024"));
        assert_eq!((i[0].credit.licence.as_str(), i[0].credit.source.as_str()), ("MIT", "Iconify (Phosphor)"));

        let ph = br#"{"rock_wall":{"name":"Rock Wall","tags":["stone"],"categories":["rock"],"download_count":5,"authors":{"Rob":"All"}},"red_brick":{"name":"Red Brick","tags":["wall"],"categories":["brick"],"download_count":50,"authors":{}},"oak":{"name":"Oak","tags":["wood"],"download_count":9}}"#;
        let found = Source::PolyHaven.parse_page(Kind::Texture, "wall", 1, ph).unwrap();
        assert_eq!(found.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), ["red_brick", "rock_wall"], "matches, most downloaded first");
        assert!(Source::PolyHaven.parse_page(Kind::Texture, "wall", 2, ph).unwrap().is_empty());
        let files = br#"{"Diffuse":{"2k":{"jpg":{"size":2802005,"url":"https://dl.polyhaven.org/x_diff_2k.jpg"}}}}"#;
        assert_eq!(pick_file(&found[0], files).unwrap(), ("https://dl.polyhaven.org/x_diff_2k.jpg".into(), Some(2802005)));

        let tree = br#"{"tree":[{"path":"Dancer/Aurora","type":"tree"},{"path":"Dancer/Aurora/$$$ Royal - Mashup (257).milk"},{"path":"Fractal/Geiss - Spiral.milk"},{"path":"README.md"}]}"#;
        let m = Source::MilkDrop.parse_page(Kind::Visualizer, "aurora", 1, tree).unwrap();
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].title, "$$$ Royal - Mashup (257) (Dancer › Aurora)");
        assert_eq!(m[0].credit.author, "$$$ Royal");
        assert_eq!(m[0].file.as_deref(), Some("https://raw.githubusercontent.com/projectM-visualizer/presets-cream-of-the-crop/master/Dancer/Aurora/%24%24%24%20Royal%20-%20Mashup%20%28257%29.milk"));
        assert!(m[0].credit.commercial());
        assert_eq!(Source::MilkDrop.parse_page(Kind::Visualizer, "", 1, tree).unwrap().len(), 2, "no words: every preset");

        let meta = br#"{"familyMetadataList":[{"family":"Roboto","category":"Sans Serif","popularity":1,"designers":["Christian Robertson"]},{"family":"Bebas Neue","category":"Display","popularity":30,"designers":["Ryoichi Tsunekawa"]},{"family":"Lobster","category":"Display","popularity":20}]}"#;
        let f = Source::GoogleFonts.parse_page(Kind::Font, "display", 1, meta).unwrap();
        assert_eq!(f.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(), ["Lobster", "Bebas Neue"], "most popular first");
        assert_eq!(f[1].list, "https://fonts.google.com/download/list?family=Bebas%20Neue");
        let list = br#")]}'
{"manifest":{"files":[{"filename":"OFL.txt"}],"fileRefs":[{"filename":"static/Roboto-Bold.ttf","url":"https://fonts.gstatic.com/b.ttf"},{"filename":"Roboto[wdth,wght].ttf","url":"https://fonts.gstatic.com/v.ttf"},{"filename":"static/Roboto-Regular.ttf","url":"https://fonts.gstatic.com/r.ttf"}]}}"#;
        assert_eq!(font_files(list).unwrap(), [("Roboto-Regular.ttf".to_string(), "https://fonts.gstatic.com/r.ttf".to_string()), ("Roboto-Bold.ttf".into(), "https://fonts.gstatic.com/b.ttf".into())]);
        let variable = br#"{"manifest":{"fileRefs":[{"filename":"Inter[opsz,wght].ttf","url":"https://fonts.gstatic.com/i.ttf"}]}}"#;
        assert_eq!(font_files(variable).unwrap().len(), 1, "a variable font alone");

        let isf = br#"{"numberOfResults":2,"shaders":[{"id":"5e7a","title":"Square Tunnel.fs","username":"joshpbatty","thumbnailCloudinaryId":"z9tk","private":false},{"id":"x","title":"Mine","private":true}]}"#;
        let sh = Source::IsfOnline.parse(Kind::Shader, isf).unwrap();
        assert_eq!(sh.len(), 1, "private ones are left out");
        assert_eq!((sh[0].title.as_str(), sh[0].thumb.as_str()), ("Square Tunnel", "https://res.cloudinary.com/hrlz5rsqo/image/upload/w_256/z9tk.jpg"));
        assert!(!sh[0].credit.commercial(), "no licence stated: not for paid work");
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
