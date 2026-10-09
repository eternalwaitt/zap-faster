//! Native bounded gallery, adapted from crmne/zapfast#264's media grid.
use super::widgets;
use crate::app::App;
use crate::model::{Action, ChatMedia, Content, MediaTab, Message};
use crate::theme::{self, Icon};
use egui::{Color32, Rect, Sense, Vec2, pos2, vec2};
const MARGIN: f32 = 4.0;
const GAP: f32 = 4.0;

pub(super) fn uri(app: &App, message: &Message) -> String {
    use std::hash::{Hash, Hasher};
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    (
        &app.account().id,
        &message.chat,
        &message.id,
        &message.thumbnail,
    )
        .hash(&mut hash);
    format!("bytes://gallery-{:x}.jpg", hash.finish())
}

pub fn show(app: &mut App, ui: &mut egui::Ui, chat: &str) {
    if !app.gallery_accessible(chat) {
        return;
    }
    ui.horizontal(|ui| {
        if ui
            .button(crate::i18n::gettext(app.locale, "Back"))
            .clicked()
        {
            app.actions.push(Action::GalleryTab(None));
        }
        ui.heading(crate::i18n::gettext(app.locale, "Media, links and docs"));
    });
    ui.horizontal(|ui| {
        for (tab, label) in [
            (MediaTab::Media, crate::i18n::gettext(app.locale, "Media")),
            (
                MediaTab::Docs,
                crate::i18n::gettext(app.locale, "Documents"),
            ),
            (MediaTab::Links, crate::i18n::gettext(app.locale, "Links")),
        ] {
            if ui
                .selectable_label(app.gallery.tab == Some(tab), label)
                .clicked()
            {
                app.actions.push(Action::GalleryTab(Some(tab)));
            }
        }
        if ui
            .small_button(crate::i18n::gettext(app.locale, "Refresh"))
            .clicked()
        {
            app.actions.push(Action::RefreshGallery);
        }
    });
    let listing = app
        .gallery
        .listing
        .take()
        .filter(|listing| listing.chat == chat);
    let Some(listing) = listing else {
        if app.gallery.failed {
            ui.label(crate::i18n::gettext(
                app.locale,
                "Could not load this chat's media.",
            ));
        } else {
            theme::spinner(ui, 22.0, app.palette.secondary);
        }
        return;
    };
    let tab = app.gallery.tab.unwrap_or(MediaTab::Media);
    let truncated = match tab {
        MediaTab::Media => listing.media_truncated,
        MediaTab::Docs => listing.docs_truncated,
        MediaTab::Links => listing.links_truncated,
    };
    if truncated {
        ui.label(crate::i18n::gettext(
            app.locale,
            "This list is limited. Use chat search for older items.",
        ));
    }
    let height = (ui.ctx().content_rect().height() - 180.0).clamp(100.0, 600.0);
    // A fresh child starts below the header. show_rows uses its max_rect origin.
    ui.allocate_ui_with_layout(
        vec2(ui.available_width(), height),
        egui::Layout::top_down(egui::Align::Min),
        |ui| match tab {
            MediaTab::Media => {
                let uris: Vec<_> = listing.media.iter().map(|row| uri(app, row)).collect();
                media_grid(app, ui, &listing, &uris, true);
            }
            MediaTab::Docs => {
                egui::ScrollArea::vertical()
                    .id_salt(("gallery-docs", chat))
                    .show_rows(ui, 92.0, listing.docs.len(), |ui, range| {
                        for row in &listing.docs[range] {
                            let Content::Document {
                                file_name, media, ..
                            } = &row.content
                            else {
                                continue;
                            };
                            ui.push_id(&row.id, |ui| {
                                let hidden = app.settings.screen_privacy.hides(
                                    crate::settings::ScreenPrivacyWhat::Message,
                                    ui.rect_contains_pointer(Rect::from_min_size(
                                        ui.cursor().min,
                                        vec2(ui.available_width(), 64.0),
                                    )),
                                );
                                let label = if hidden {
                                    crate::i18n::gettext(app.locale, "Document").into_owned()
                                } else {
                                    format!("{}  {}", file_name, crate::util::bytes(media.size))
                                };
                                if ui.button(label).clicked() {
                                    app.actions.push(Action::CloseDialog);
                                    app.actions.push(Action::OpenMessage {
                                        chat: row.chat.clone(),
                                        message: row.id.clone(),
                                    });
                                }
                                if let crate::model::MediaState::Transferring { bytes, total } =
                                    media.state
                                {
                                    let fraction = total
                                        .filter(|total| *total > 0)
                                        .map_or(0.0, |total| bytes as f32 / total as f32);
                                    ui.add(
                                        egui::ProgressBar::new(fraction.min(1.0))
                                            .text(crate::util::bytes(bytes)),
                                    );
                                    if ui
                                        .small_button(crate::i18n::gettext(app.locale, "Cancel"))
                                        .clicked()
                                    {
                                        app.actions.push(Action::CancelDownload {
                                            chat: row.chat.clone(),
                                            message: row.id.clone(),
                                            card: None,
                                        });
                                    }
                                } else if matches!(
                                    media.state,
                                    crate::model::MediaState::Downloading
                                ) {
                                    theme::spinner(ui, 16.0, app.palette.accent);
                                } else if let Some(path) = &media.path {
                                    if ui
                                        .small_button(crate::i18n::gettext(app.locale, "Save as…"))
                                        .clicked()
                                    {
                                        app.actions.push(Action::SaveAttachmentAs {
                                            path: path.clone(),
                                            name: file_name.clone(),
                                        });
                                    }
                                } else if ui
                                    .small_button(crate::i18n::gettext(app.locale, "Download"))
                                    .clicked()
                                {
                                    app.actions.push(Action::Download {
                                        card: None,
                                        chat: row.chat.clone(),
                                        message: row.id.clone(),
                                    });
                                }
                            });
                        }
                    });
                if listing.docs.is_empty() {
                    ui.label(crate::i18n::gettext(app.locale, "No documents"));
                }
            }
            MediaTab::Links => {
                egui::ScrollArea::vertical()
                    .id_salt(("gallery-links", chat))
                    .show_rows(ui, 56.0, listing.links.len(), |ui, range| {
                        for link in &listing.links[range] {
                            let hidden = app.settings.screen_privacy.hides(
                                crate::settings::ScreenPrivacyWhat::Message,
                                ui.rect_contains_pointer(Rect::from_min_size(
                                    ui.cursor().min,
                                    vec2(ui.available_width(), 56.0),
                                )),
                            );
                            let label = if hidden {
                                crate::i18n::gettext(app.locale, "Link").into_owned()
                            } else {
                                link.title.as_deref().unwrap_or(&link.url).to_owned()
                            };
                            if ui.button(label).clicked() {
                                app.actions.push(Action::OpenUrl(link.url.clone()));
                            }
                        }
                    });
                if listing.links.is_empty() {
                    ui.label(crate::i18n::gettext(app.locale, "No links"));
                }
            }
        },
    );
    app.gallery.listing = Some(listing);
}

/// The chat's pictures and videos, newest first, in square cells as wide as
/// the panel allows. Only the rows on screen are laid out, so only their
/// thumbnails are decoded.
fn media_grid(
    app: &mut App,
    ui: &mut egui::Ui,
    listing: &ChatMedia,
    uris: &[String],
    overlay: bool,
) {
    let palette = app.palette;
    if listing.media.is_empty() {
        widgets::empty_state(
            ui,
            &palette,
            Icon::Image,
            &crate::i18n::gettext(app.locale, "No media"),
            &crate::i18n::gettext(
                app.locale,
                "Photos and videos shared in this chat will show here.",
            ),
        );
        return;
    }
    let width = ui.available_width() - 2.0 * MARGIN;
    let columns = if width >= 4.0 * 84.0 + 3.0 * GAP {
        4
    } else {
        3
    };
    let cell = (width - (columns - 1) as f32 * GAP) / columns as f32;
    let rows = listing.media.len().div_ceil(columns);
    ui.add_space(6.0);
    // `show_rows` reads the spacing from outside its closure.
    ui.spacing_mut().item_spacing = vec2(GAP, GAP);
    egui::ScrollArea::vertical()
        .id_salt(("info-media-grid", listing.chat.as_str()))
        .auto_shrink([false, false])
        .show_rows(ui, cell + GAP, rows, |ui, range| {
            for row in range {
                ui.horizontal(|ui| {
                    ui.add_space(MARGIN);
                    for column in 0..columns {
                        let index = row * columns + column;
                        let Some(message) = listing.media.get(index) else {
                            break;
                        };
                        let (rect, response) =
                            ui.allocate_exact_size(Vec2::splat(cell), Sense::click());
                        media_cell(
                            app,
                            ui,
                            rect,
                            response,
                            message,
                            uris.get(index).map(String::as_str),
                            overlay,
                        );
                    }
                });
            }
        });
}

/// One square picture or video: its preview cropped to the square, with a
/// play mark and the length on a video. Clicking it goes to the message.
fn media_cell(
    app: &mut App,
    ui: &mut egui::Ui,
    rect: Rect,
    response: egui::Response,
    message: &Message,
    uri: Option<&str>,
    _overlay: bool,
) {
    let palette = app.palette;
    let video = matches!(message.content, Content::Video { .. });
    let seconds = match &message.content {
        Content::Video { seconds, .. } => *seconds,
        _ => None,
    };
    let stamp = crate::util::chat_stamp(app.locale, message.timestamp);
    response.widget_info(|| {
        let what = if video {
            crate::i18n::gettext(app.locale, "Video")
        } else {
            crate::i18n::gettext(app.locale, "Photo")
        };
        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, format!("{what}, {stamp}"))
    });
    let hidden = app.settings.screen_privacy.hides(
        crate::settings::ScreenPrivacyWhat::Media,
        response.hovered(),
    );
    if ui.is_rect_visible(rect) && !hidden {
        if !thumbnail(ui, &palette, rect, message, uri) {
            ui.painter().rect_filled(rect, 6.0, palette.surface);
            let icon = if video { Icon::Video } else { Icon::Image };
            theme::paint_icon(ui, icon, rect, 22.0, palette.dim);
        }
        if video {
            let disc = Rect::from_center_size(rect.center(), Vec2::splat(30.0));
            ui.painter()
                .circle_filled(disc.center(), 15.0, Color32::from_black_alpha(140));
            theme::paint_icon(ui, Icon::Play, disc, 14.0, Color32::WHITE);
            if let Some(seconds) = seconds {
                let galley = ui.painter().layout_no_wrap(
                    crate::util::duration(seconds),
                    theme::medium(11.0),
                    Color32::WHITE,
                );
                let pad = vec2(4.0, 2.0);
                let label = Rect::from_min_max(
                    rect.max - galley.size() - pad * 2.0 - Vec2::splat(4.0),
                    rect.max - Vec2::splat(4.0),
                );
                ui.painter()
                    .rect_filled(label, 3.0, Color32::from_black_alpha(140));
                ui.painter().galley(label.min + pad, galley, Color32::WHITE);
            }
        }
        if response.hovered() {
            ui.painter()
                .rect_filled(rect, 6.0, Color32::from_white_alpha(24));
        }
    }
    if hidden {
        widgets::privacy_cover(ui, rect, palette.surface_hover, 6.0);
    }
    if response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
    {
        app.actions.push(Action::PreviewMedia {
            chat: message.chat.clone(),
            message: message.id.clone(),
        });
    }
}

/// Paints the message's preview cropped to fill `rect`, and reports whether
/// there was one. The archived JPEG preview needs no download; a downloaded
/// picture without one goes through egui's file loader, as in the bubble. A
/// video without one keeps its placeholder: its file is not a picture.
pub(super) fn thumbnail(
    ui: &egui::Ui,
    palette: &theme::Palette,
    rect: Rect,
    message: &Message,
    uri: Option<&str>,
) -> bool {
    let image = match (&message.thumbnail, uri, &message.content) {
        (Some(bytes), Some(uri), _) if bytes.len() <= 256 * 1024 => {
            crate::image_cache::include(ui.ctx(), uri.to_owned(), bytes);
            egui::Image::new(uri.to_owned())
        }
        _ => return false,
    };
    // The texture's shape is known once it is decoded; until then the cell
    // is a plain surface rather than a stretched placeholder.
    let Ok(egui::load::TexturePoll::Ready { texture }) = image.load_for_size(ui.ctx(), rect.size())
    else {
        ui.painter().rect_filled(rect, 6.0, palette.surface);
        return true;
    };
    // Cover the square: the longer side is cropped at both ends.
    let (width, height) = (texture.size.x.max(1.0), texture.size.y.max(1.0));
    let uv = if width > height {
        let cut = (1.0 - height / width) / 2.0;
        Rect::from_min_max(pos2(cut, 0.0), pos2(1.0 - cut, 1.0))
    } else {
        let cut = (1.0 - width / height) / 2.0;
        Rect::from_min_max(pos2(0.0, cut), pos2(1.0, 1.0 - cut))
    };
    image.uv(uv).corner_radius(6.0).paint_at(ui, rect);
    true
}
