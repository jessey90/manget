use reqwest::IntoUrl;
use scraper::{Html, Selector};

use crate::{download::DownloadItem, manga::Chapter};

#[derive(Debug, thiserror::Error)]
pub enum NettruyenError {
    #[error(transparent)]
    RequestError(#[from] reqwest::Error),
    #[error("Parse error: {0}")]
    ParseError(&'static str),
}

#[derive(Debug)]
pub struct NettruyenChapter {
    url: String,
    manga: String,
    chapter: String,
    pages: Vec<DownloadItem>,
    referer: Option<String>,
}

impl NettruyenChapter {
    pub async fn from_url(url: impl IntoUrl + Clone + ToString) -> Result<Self, NettruyenError> {
        let response = reqwest::Client::new()
            .get(url.clone())
            .header("User-Agent", "Manget")
            .send()
            .await?
            .error_for_status()?;
        let html_content = response.text().await?;

        let html = Html::parse_document(&html_content);
        // Old layout: <h1 class="txt-primary">Manga<span>- Chapter</span></h1>
        // New truyenqq layout: <h1 class="current-book"><a>Manga : Chapter</a></h1>
        let old_title = Selector::parse("h1.txt-primary").unwrap();
        let new_title = Selector::parse("h1.current-book").unwrap();

        let mut manga = String::new();
        let mut chapter = String::new();
        if let Some(h1_elm) = html.select(&old_title).next() {
            let mut text_iter = h1_elm.text();
            // find manga title
            for _ in 0..10 {
                if let Some(s) = text_iter.next() {
                    if !s.trim().is_empty() {
                        manga = s.trim().to_string();
                        break;
                    }
                }
            }
            // find chapter title
            for _ in 0..10 {
                if let Some(s) = text_iter.next() {
                    if !s.trim().is_empty() {
                        chapter = s.trim().trim_start_matches("- ").to_string();
                        break;
                    }
                }
            }
        } else if let Some(h1_elm) = html.select(&new_title).next() {
            let full: String = h1_elm.text().collect::<String>();
            let full = full.trim();
            match full.rsplit_once(" : ") {
                Some((m, c)) => {
                    manga = m.trim().to_string();
                    chapter = c.trim().to_string();
                }
                None => {
                    manga = full.to_string();
                    chapter = full.to_string();
                }
            }
        } else {
            return Err(NettruyenError::ParseError("cannot find title"));
        }

        let img_selector = Selector::parse("div.page-chapter > img").unwrap();
        // New truyenqq layout: <img class="lazy" data-src="https://.../chapters/..." src="/images/loading.svg">
        let lazy_selector = Selector::parse("img.lazy[data-src]").unwrap();
        let mut img_elems: Vec<_> = html.select(&img_selector).collect();
        let lazy_layout = img_elems.is_empty();
        if lazy_layout {
            img_elems = html
                .select(&lazy_selector)
                .filter(|e| {
                    e.value()
                        .attr("data-src")
                        .map(|x| x.contains("/chapters/"))
                        .unwrap_or(false)
                })
                .collect();
        }
        let mut pages = Vec::new();
        let mut has_referer = true;
        for (i, img_elem) in img_elems.into_iter().enumerate() {
            if img_elem.value().attr("referrerpolicy") == Some("no-referrer") {
                has_referer = false;
            }
            let src: &str;
            if lazy_layout {
                // src is only a placeholder in the lazy layout
                match img_elem.value().attr("data-src") {
                    Some(s) => src = s,
                    None => continue,
                }
            } else if let Some(s) = img_elem.value().attr("src") {
                src = s;
            } else if let Some(s) = img_elem.value().attr("data-sv1") {
                src = s;
            } else if let Some(s) = img_elem.value().attr("data-src") {
                src = s;
            } else {
                continue;
            }
            let src = if src.starts_with("http") {
                src.to_string()
            } else {
                format!("https:{}", src)
            };
            let alt = img_elem.value().attr("data-cdn").map(|x| {
                if x.starts_with("http") {
                    x.to_string()
                } else {
                    format!("https:{}", x)
                }
            });
            let ext = if src.contains(".png") {
                "png"
            } else if src.contains(".webp") {
                "webp"
            } else {
                "jpg"
            };
            pages.push(
                DownloadItem::new(src, Some(&format!("page_{:02}.{}", i, ext))).add_option_url(alt),
            );
        }

        let url = url.into_url()?;
        let referer = if has_referer {
            let domain = url.domain().unwrap_or_default();
            let scheme = url.scheme();
            Some(format!("{}://{}/", scheme, domain))
        } else {
            None
        };

        Ok(Self {
            url: url.to_string(),
            manga,
            chapter,
            pages,
            referer,
        })
    }
}

impl Chapter for NettruyenChapter {
    fn url(&self) -> String {
        self.url.to_string()
    }

    fn manga(&self) -> String {
        self.manga.clone()
    }

    fn chapter(&self) -> String {
        self.chapter.clone()
    }

    fn pages_download_info(&self) -> &Vec<DownloadItem> {
        &self.pages
    }

    fn referer(&self) -> Option<String> {
        self.referer.clone()
    }
}

#[cfg(test)]
#[tokio::test]
async fn test_build_nettruyenus_chapter() {
    let chapter = NettruyenChapter::from_url(
        "https://www.nettruyenus.com/truyen-tranh/cuon-sach-cua-lagier/chap-77/1062446",
    )
    .await
    .unwrap();
    dbg!(&chapter);
    assert!(chapter.manga.to_lowercase().contains("lagier"));
    assert!(chapter.chapter.contains("77"));
    assert!(!chapter.pages.is_empty());
}

#[cfg(test)]
#[tokio::test]
async fn test_build_nettruyenco_chapter() {
    let chapter = NettruyenChapter::from_url(
        "https://nettruyenco.vn/truyen-tranh/grand-blue-co-gai-thich-lan/chuong-85/749049",
    )
    .await
    .unwrap();
    dbg!(&chapter);
    assert!(chapter.manga.to_lowercase().contains("grand blue"));
    assert!(chapter.chapter.contains("85"));
    assert!(!chapter.pages.is_empty());
}
