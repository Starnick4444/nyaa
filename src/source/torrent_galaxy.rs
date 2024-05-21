use std::{cmp::max, collections::HashMap, error::Error};

use ratatui::{
    layout::{Alignment, Constraint},
    style::{Color, Stylize},
};
use reqwest::{StatusCode, Url};
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use urlencoding::encode;

use crate::{
    app::{Context, Widgets},
    cats, collection,
    config::Config,
    popup_enum,
    results::{ResultColumn, ResultHeader, ResultRow, ResultTable},
    util::{
        conv::{shorten_number, to_bytes},
        html::{attr, inner},
    },
    widget::EnumIter as _,
};

use super::{add_protocol, Item, ItemType, Source, SourceInfo};

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct TgxConfig {
    pub base_url: String,
    pub default_sort: TgxSort,
    pub default_filter: TgxFilter,
    pub default_category: String,
    pub default_search: String,
}

impl Default for TgxConfig {
    fn default() -> Self {
        Self {
            base_url: "https://torrentgalaxy.to/".to_owned(),
            default_sort: TgxSort::Date,
            default_filter: TgxFilter::NoFilter,
            default_category: "AllCategories".to_owned(),
            default_search: Default::default(),
        }
    }
}

popup_enum! {
    TgxSort;
    (0, Date, "Date");
    (1, Seeders, "Seeders");
    (2, Leechers, "Leechers");
    (3, Size, "Size");
    (4, Name, "Name");
}

popup_enum! {
    TgxFilter;
    #[allow(clippy::enum_variant_names)]
    (0, NoFilter, "NoFilter");
    (1, OnlineStreams, "Filter online streams");
    (2, ExcludeXXX, "Exclude XXX");
    (3, NoWildcard, "No wildcard");
}

pub struct TorrentGalaxyHtmlSource;

fn get_lang(full_name: String) -> String {
    match full_name.as_str() {
        "English" => "en",
        "French" => "fr",
        "German" => "de",
        "Italian" => "it",
        "Japanese" => "jp",
        "Spanish" => "es",
        "Russian" => "ru",
        "Norwegian" => "no",
        "Hindi" => "hi",
        "Korean" => "ko",
        "Danish" => "da",
        "Dutch" => "nl",
        "Chinese" => "zh",
        "Portuguese" => "pt",
        "Polish" => "pl",
        "Turkish" => "tr",
        "Telugu" => "te",
        "Swedish" => "sv",
        "Czech" => "cs",
        "Arabic" => "ar",
        "Romanian" => "ro",
        "Bengali" => "bn",
        "Urdu" => "ur",
        "Thai" => "th",
        "Tamil" => "ta",
        "Croatian" => "hr",
        _ => "??",
    }
    .to_owned()
}

fn get_status_color(status: String) -> Option<Color> {
    match status.as_str() {
        "Trial Uploader" => Some(Color::Magenta),
        "Trusted Uploader" => Some(Color::LightGreen),
        "Trusted User" => Some(Color::Cyan),
        "Moderator" => Some(Color::Red),
        "Admin" => Some(Color::Yellow),
        "Torrent Officer" => Some(Color::LightYellow),
        "Verified Uploader" => Some(Color::LightBlue),
        _ => None,
    }
}

impl Source for TorrentGalaxyHtmlSource {
    async fn filter(
        client: &reqwest::Client,
        ctx: &mut Context,
        w: &Widgets,
    ) -> Result<ResultTable, Box<dyn Error>> {
        TorrentGalaxyHtmlSource::search(client, ctx, w).await
    }
    async fn categorize(
        client: &reqwest::Client,
        ctx: &mut Context,
        w: &Widgets,
    ) -> Result<ResultTable, Box<dyn Error>> {
        TorrentGalaxyHtmlSource::search(client, ctx, w).await
    }
    async fn sort(
        client: &reqwest::Client,
        ctx: &mut Context,
        w: &Widgets,
    ) -> Result<ResultTable, Box<dyn Error>> {
        TorrentGalaxyHtmlSource::search(client, ctx, w).await
    }
    async fn search(
        client: &reqwest::Client,
        ctx: &mut Context,
        w: &Widgets,
    ) -> Result<ResultTable, Box<dyn Error>> {
        let tgx = ctx.config.sources.tgx.to_owned().unwrap_or_default();
        let base_url = Url::parse(&add_protocol(tgx.base_url, true))?.join("torrents.php")?;
        let query = encode(&w.search.input.input);

        let sort = match TgxSort::try_from(w.sort.selected.sort) {
            Ok(TgxSort::Date) => "&sort=id",
            Ok(TgxSort::Seeders) => "&sort=seeders",
            Ok(TgxSort::Leechers) => "&sort=leechers",
            Ok(TgxSort::Size) => "&sort=size",
            Ok(TgxSort::Name) => "&sort=name",
            _ => "",
        };
        let ord = format!("&order={}", w.sort.selected.dir.to_url());
        let filter = match TgxFilter::try_from(w.filter.selected) {
            Ok(TgxFilter::OnlineStreams) => "&filterstream=1",
            Ok(TgxFilter::ExcludeXXX) => "&nox=2&nox=1",
            Ok(TgxFilter::NoWildcard) => "&nowildcard=1",
            _ => "",
        };
        let cat = match w.category.selected {
            0 => "".to_owned(),
            x => format!("&c{}=1", x),
        };

        let q = format!(
            "search={}&page={}{}{}{}{}",
            query,
            ctx.page - 1,
            filter,
            cat,
            sort,
            ord
        );
        let mut url = base_url.clone();
        url.set_query(Some(&q));

        let response = client.get(url.to_owned()).send().await?;
        if response.status() != StatusCode::OK {
            // Throw error if response code is not OK
            let code = response.status().as_u16();
            return Err(format!("{}\nInvalid repsponse code: {}", url, code).into());
        }
        let content = response.text().await?;
        let doc = Html::parse_document(&content);

        let item_sel = &Selector::parse("div.tgxtablerow")?;
        let title_sel = &Selector::parse("div.tgxtablecell:nth-of-type(4) > div > a.txlight")?;
        let cat_sel = &Selector::parse("div.tgxtablecell:nth-of-type(1) > a")?;
        let date_sel = &Selector::parse("div.tgxtablecell:nth-of-type(12)")?;
        let seed_sel =
            &Selector::parse("div.tgxtablecell:nth-of-type(11) > span > font:first-of-type > b")?;
        let leech_sel =
            &Selector::parse("div.tgxtablecell:nth-of-type(11) > span > font:last-of-type > b")?;
        let size_sel = &Selector::parse("div.tgxtablecell:nth-of-type(8) > span")?;
        let trust_sel = &Selector::parse("div.tgxtablecell:nth-of-type(2) > i")?;
        let views_sel = &Selector::parse("div.tgxtablecell:nth-of-type(10) > span > font > b")?;
        let torrent_sel = &Selector::parse("div.tgxtablecell:nth-of-type(5) > a:first-of-type")?;
        let magnet_sel = &Selector::parse("div.tgxtablecell:nth-of-type(5) > a:last-of-type")?;
        let lang_sel = &Selector::parse("div.tgxtablecell:nth-of-type(3) > img")?;
        let uploader_sel = &Selector::parse("div.tgxtablecell:nth-of-type(7) > span > a > span")?;
        let uploader_status_sel = &Selector::parse("div.tgxtablecell:nth-of-type(7) > span > a")?;

        let pagination_sel = &Selector::parse("div#filterbox2 > span.badge")?;

        let items = doc
            .select(item_sel)
            .enumerate()
            .map(|(i, e)| {
                let cat_id = attr(e, cat_sel, "href")
                    .rsplit_once('=')
                    .map(|v| v.1)
                    .and_then(|v| v.parse::<usize>().ok())
                    .unwrap_or_default();
                let icon = Self::info().entry_from_id(cat_id).icon;
                let date = e
                    .select(date_sel)
                    .nth(0)
                    .map(|e| e.text().collect())
                    .unwrap_or_default();
                let seeders = inner(e, seed_sel, "0")
                    .chars()
                    .filter(char::is_ascii_digit)
                    .collect::<String>()
                    .parse::<u32>()
                    .unwrap_or_default();
                let leechers = inner(e, leech_sel, "0")
                    .chars()
                    .filter(char::is_ascii_digit)
                    .collect::<String>()
                    .parse::<u32>()
                    .unwrap_or_default();
                let views = inner(e, views_sel, "0")
                    .chars()
                    .filter(char::is_ascii_digit)
                    .collect::<String>()
                    .parse::<u32>()
                    .unwrap_or_default();
                let mut size = inner(e, size_sel, "0 MB");

                // Convert numbers like 1,015 KB => 1.01 MB
                if let Some((x, y)) = size.split_once(',') {
                    if let Some((y, unit)) = y.split_once(' ') {
                        let y = y.get(0..2).unwrap_or("00");
                        // find next unit up
                        let unit = match unit.to_lowercase().as_str() {
                            "b" => "kB",
                            "kb" => "MB",
                            "mb" => "GB",
                            "gb" => "TB",
                            _ => "??",
                        };
                        size = format!("{}.{} {}", x, y, unit);
                    }
                }

                let item_type = match e
                    .select(trust_sel)
                    .nth(0)
                    .map(|v| v.value().classes().any(|e| e == "fa-check"))
                    .unwrap_or(false)
                {
                    true => ItemType::None,
                    false => ItemType::Remake,
                };

                let torrent_link = attr(e, torrent_sel, "href");
                let torrent_link = base_url
                    .join(&torrent_link)
                    .map(|u| u.to_string())
                    .unwrap_or_default();
                let magnet_link = attr(e, magnet_sel, "href");
                let post_link = attr(e, title_sel, "href");
                let post_link = base_url
                    .join(&post_link)
                    .map(|u| u.to_string())
                    .unwrap_or_default();
                let hash = torrent_link.split('/').nth(4).unwrap_or("unknown");
                let file_name = format!("{}.torrent", hash);

                let extra: HashMap<String, String> = collection![
                    "uploader".to_owned() => inner(e, uploader_sel, "???"),
                    "uploader_status".to_owned() => attr(e, uploader_status_sel, "title"),
                    "lang".to_owned() => attr(e, lang_sel, "title"),
                ];

                Item {
                    id: i,
                    date,
                    seeders,
                    leechers,
                    downloads: views,
                    bytes: to_bytes(&size),
                    size,
                    title: attr(e, title_sel, "title"),
                    torrent_link,
                    magnet_link,
                    post_link,
                    file_name,
                    category: cat_id,
                    icon,
                    item_type,
                    extra,
                }
            })
            .collect::<Vec<Item>>();

        ctx.last_page = 50;
        ctx.total_results = 2500;
        if let Some(pagination) = doc.select(pagination_sel).nth(0) {
            if let Ok(num_results) = pagination
                .inner_html()
                .chars()
                .filter(|c| c.is_ascii_digit())
                .collect::<String>()
                .parse::<usize>()
            {
                if num_results != 0 || items.is_empty() {
                    ctx.last_page = (num_results + 49) / 50;
                    ctx.total_results = num_results;
                }
            }
        }

        let raw_date_width = items.iter().map(|i| i.date.len()).max().unwrap_or_default() as u16;
        let date_width = max(raw_date_width, 6);

        let raw_uploader_width = items
            .iter()
            .map(|i| i.extra.get("uploader").map(|u| u.len()).unwrap_or(0))
            .max()
            .unwrap_or_default() as u16;
        let uploader_width = max(raw_uploader_width, 8);

        let header = ResultHeader::new([
            ResultColumn::Normal("Cat".to_owned(), Constraint::Length(3)),
            ResultColumn::Normal("".to_owned(), Constraint::Length(2)),
            ResultColumn::Normal("Name".to_owned(), Constraint::Min(3)),
            ResultColumn::Normal("Uploader".to_owned(), Constraint::Length(uploader_width)),
            ResultColumn::Sorted("Size".to_owned(), 9, TgxSort::Size as u32),
            ResultColumn::Sorted("Date".to_owned(), date_width, TgxSort::Date as u32),
            ResultColumn::Sorted("".to_owned(), 4, TgxSort::Seeders as u32),
            ResultColumn::Sorted("".to_owned(), 4, TgxSort::Leechers as u32),
            ResultColumn::Normal("  󰈈".to_owned(), Constraint::Length(5)),
        ]);
        let binding = header.get_binding();
        let align = [
            Alignment::Left,
            Alignment::Left,
            Alignment::Left,
            Alignment::Left,
            Alignment::Right,
            Alignment::Left,
            Alignment::Right,
            Alignment::Right,
            Alignment::Left,
        ];
        let rows: Vec<ResultRow> = items
            .iter()
            .map(|item| {
                ResultRow::new([
                    item.icon.label.fg(item.icon.color),
                    item.extra
                        .get("lang")
                        .map(|l| get_lang(l.to_owned()))
                        .unwrap_or("??".to_owned())
                        .into(),
                    item.title.to_owned().fg(match item.item_type {
                        ItemType::Trusted => ctx.theme.trusted,
                        ItemType::Remake => ctx.theme.remake,
                        ItemType::None => ctx.theme.fg,
                    }),
                    item.extra
                        .get("uploader")
                        .unwrap_or(&"???".to_owned())
                        .to_owned()
                        .fg(item
                            .extra
                            .get("uploader_status")
                            .and_then(|u| get_status_color(u.to_owned()))
                            .unwrap_or(ctx.theme.fg)),
                    item.size.clone().into(),
                    item.date.clone().into(),
                    item.seeders.to_string().fg(ctx.theme.trusted),
                    item.leechers.to_string().fg(ctx.theme.remake),
                    shorten_number(item.downloads).into(),
                ])
                .aligned(align, &binding)
                .fg(ctx.theme.fg)
            })
            .collect();

        Ok(ResultTable {
            headers: header.get_row(w.sort.selected.dir, w.sort.selected.sort as u32),
            rows,
            binding,
            items,
        })
    }

    fn info() -> SourceInfo {
        let cats = cats! {
            "All Categories" => {
                0 => ("---", "All Categories", "AllCategories", White);
            }
            "Movies" => {
                3 => ("4kM", "4K UHD Movies", "4kMovies", LightMagenta);
                46 => ("Bly", "Bollywood", "Bollywood Movies", Green);
                45 => ("Cam", "Cam/TS", "CamMovies", LightCyan);
                42 => ("HdM", "HD Movies", "HdMovies", LightBlue);
                4 => ("PkM", "Movie Packs", "PackMovies", Magenta);
                1 => ("SdM", "SD Movies", "SdMovies", Yellow);
            }
            "TV" => {
                41 => ("HdT", "TV HD", "HdTV", Green);
                5 => ("SdT", "TV SD", "SdTV", LightCyan);
                11 => ("4kT", "TV 4k", "4kTV", LightMagenta);
                6 => ("PkT", "TV Packs", "PacksTV", Blue);
                7 => ("Spo", "Sports", "SportsTV", LightGreen);
            }
            "Anime" => {
                28 => ("Ani", "All Anime", "Anime", LightMagenta);
            }
            "Apps" => {
                20 => ("Mob", "Mobile Apps", "AppsMobile", LightGreen);
                21 => ("App", "Other Apps", "AppsOther", Magenta);
                18 => ("Win", "Windows Apps", "AppsWindows", LightCyan);
            }
            "Books" => {
                13 => ("Abk", "Audiobooks", "Audiobooks", Yellow);
                19 => ("Com", "Comics", "Comics", LightGreen);
                12 => ("Ebk", "Ebooks", "Ebooks", Green);
                14 => ("Edu", "Educational", "Educational", Yellow);
                15 => ("Mag", "Magazines", "Magazines", Green);
            }
            "Documentaries" => {
                9 => ("Doc", "All Documentaries", "Documentaries", LightYellow);
            }
            "Games" => {
                10 => ("Wgm", "Windows Games", "WindowsGames", LightCyan);
                43 => ("Ogm", "Other Games", "OtherGames", Yellow);
            }
            "Music" => {
                22 => ("Alb", "Music Albums", "AlbumsMusic", Cyan);
                26 => ("Dis", "Music Discography", "DiscographyMusic", Magenta);
                23 => ("Los", "Music Lossless", "LosslessMusic", LightBlue);
                25 => ("MV ", "Music Video", "MusicVideo", Green);
                24 => ("Sin", "Music Singles", "SinglesMusic", LightYellow);
            }
            "Other" => {
                17 => ("Aud", "Other Audio", "AudioOther", LightGreen);
                40 => ("Pic", "Other Pictures", "PicturesOther", Green);
                37 => ("Tra", "Other Training", "TrainingOther", LightBlue);
                33 => ("Oth", "Other", "Other", Yellow);
            }
            "XXX" => {
                48 => ("4kX", "XXX 4k", "4kXXX", Red);
                35 => ("HdX", "XXX HD", "HdXXX", Red);
                47 => ("MsX", "XXX Misc", "MiscXXX", Red);
                34 => ("SdX", "XXX SD", "SdXXX", Red);
            }
        };
        SourceInfo {
            cats,
            filters: TgxFilter::iter().map(|f| f.to_string()).collect(),
            sorts: TgxSort::iter().map(|item| item.to_string()).collect(),
        }
    }

    fn load_config(ctx: &mut Context) {
        if ctx.config.sources.tgx.is_none() {
            ctx.config.sources.tgx = Some(TgxConfig::default());
        }
    }

    fn default_category(cfg: &Config) -> usize {
        let default = cfg
            .sources
            .tgx
            .to_owned()
            .unwrap_or_default()
            .default_category;
        Self::info().entry_from_cfg(&default).id
    }

    fn default_sort(cfg: &Config) -> usize {
        cfg.sources.tgx.to_owned().unwrap_or_default().default_sort as usize
    }

    fn default_filter(cfg: &Config) -> usize {
        cfg.sources
            .tgx
            .to_owned()
            .unwrap_or_default()
            .default_filter as usize
    }
}