use super::*;

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/tests/fixtures/audiobookshelf/{name}.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

#[test]
fn fixtures_decode_without_losing_native_identity() {
    let libraries: LibrariesResponse = serde_json::from_str(&fixture("libraries")).unwrap();
    assert_eq!(libraries.libraries[1].id, "lib-podcast");
    assert_eq!(libraries.libraries[1].media_type, "podcast");
    let page: ItemsResponse = serde_json::from_str(&fixture("items-page")).unwrap();
    assert_eq!((page.page, page.limit, page.total), (0, 20, 2));
    assert_eq!(page.results[0].library_item_id, "show-2");
    assert_eq!(
        page.results[0]
            .media
            .as_ref()
            .and_then(|media| media.metadata.as_ref())
            .and_then(|metadata| metadata.title.as_deref()),
        Some("Second Show")
    );
    assert_eq!(
        page.results[0]
            .media
            .as_ref()
            .and_then(|media| media.metadata.as_ref())
            .and_then(|metadata| metadata.description.as_deref()),
        Some("Second show description.")
    );
    let expanded: ExpandedWire = serde_json::from_str(&fixture("item-expanded")).unwrap();
    assert_eq!(expanded.id, "show-2");
    assert_eq!(expanded.media.unwrap().episodes.unwrap()[0].id, "episode-1");
}

#[test]
fn progress_and_shelf_fixtures_preserve_user_and_server_order() {
    let progress: ProgressResponse = serde_json::from_str(&fixture("progress")).unwrap();
    assert_eq!(progress.media_progress[0].library_item_id, "show-2");
    assert!(!progress.media_progress[0].is_finished.unwrap());
    let completed: ProgressResponse = serde_json::from_str(
            r#"{"mediaProgress":[{"libraryItemId":"show-2","episodeId":"episode-2","currentTime":120.0,"isFinished":true}]}"#,
        )
        .unwrap();
    assert_eq!(completed.media_progress[0].is_finished, Some(true));
    let shelves: Vec<ShelfWire> = serde_json::from_str(&fixture("shelves")).unwrap();
    assert_eq!(shelves[0].label, "Continue Listening");
    assert!(matches!(shelves[0].entities[0], ShelfEntryWire { .. }));
}

#[test]
fn newest_episodes_shelf_wire_carries_the_embedded_payload() {
    let shelves: Vec<ShelfWire> = serde_json::from_str(&fixture("shelves")).unwrap();
    let newest = shelves
        .iter()
        .find(|shelf| shelf.label == "Newest Episodes")
        .expect("fixture keeps the live server's recency shelf");
    let mapped: Vec<AudiobookshelfShelfEntry> = newest
        .entities
        .iter()
        .cloned()
        .map(shelf_entry_from_wire)
        .collect();
    let AudiobookshelfShelfEntry::Episode(first) = &mapped[0] else {
        panic!("first Newest episodes entry is an episode");
    };
    assert_eq!(first.library_item_id, "show-2");
    assert_eq!(first.episode_id, "episode-3");
    assert_eq!(
        first.title, "Episode Three",
        "episode title comes from recentEpisode"
    );
    assert_eq!(
        first.show_title.as_deref(),
        Some("Second Show"),
        "show title comes from media.metadata"
    );
    assert_eq!(first.author.as_deref(), Some("Jane Doe"));
    assert_eq!(
        first.duration_ticks,
        Some((1800.0 * crate::api::TICKS_PER_SECOND as f64) as u64),
        "audioFile.duration is carried as duration ticks"
    );
    assert_eq!(first.cover_path.as_deref(), Some("/api/items/show-2/cover"));
    assert_eq!(first.pub_date_secs, Some(1_700_000_000));
    assert_eq!(
        first.description.as_deref(),
        Some(
            "As Russia batters Kyiv, we take a look at the vanishing concept of the frontline.\n\
             Read more on the full story (https://example.test/story) and subscribe for & updates."
        ),
        "recentEpisode.description HTML is converted to terminal text \
         (paragraph breaks, decoded entities, link text + URL)"
    );
    let AudiobookshelfShelfEntry::Episode(second) = &mapped[1] else {
        panic!("second episode entry is an episode");
    };
    assert_eq!(second.title, "Episode Four");
    assert_eq!(
        second.pub_date_secs,
        Some(1_700_000_100),
        "string epoch publishedAt parses"
    );
    assert_eq!(second.duration_ticks, None, "missing audioFile stays None");
    assert_eq!(second.cover_path, None, "missing coverPath stays None");
    assert!(
        matches!(&mapped[0], AudiobookshelfShelfEntry::Episode(_)),
        "every Newest Episodes entry maps to an episode"
    );
}

#[test]
fn non_newest_episodes_shelves_parse_and_stay_unused() {
    // Home's Latest pill reads only the `Newest Episodes` shelf (Task 6.3);
    // every other shelf the live server returns must still parse cleanly and
    // simply never feed Home. The fixture's `Continue Listening` shelf pins
    // both the show shape and the bare (no embedded media) episode shape.
    let shelves: Vec<ShelfWire> = serde_json::from_str(&fixture("shelves")).unwrap();
    let continue_listening = shelves
        .iter()
        .find(|shelf| shelf.label == "Continue Listening")
        .expect("fixture keeps a non-recency shelf");
    let mapped: Vec<AudiobookshelfShelfEntry> = continue_listening
        .entities
        .iter()
        .cloned()
        .map(shelf_entry_from_wire)
        .collect();
    assert!(
        matches!(&mapped[0], AudiobookshelfShelfEntry::Episode(_)),
        "podcast Continue Listening entries carry a recentEpisode and map to Episode"
    );
    assert!(
        matches!(&mapped[1], AudiobookshelfShelfEntry::Show(id) if id == "missing-show"),
        "an entry with a null recentEpisode maps to a bare Show id"
    );
    let AudiobookshelfShelfEntry::Episode(first) = &mapped[0] else {
        panic!("first Continue Listening entry is an episode");
    };
    assert_eq!(first.episode_id, "episode-1");
    assert_eq!(
        first.show_title.as_deref(),
        Some("Second Show"),
        "Continue Listening entries carry embedded media like Newest Episodes"
    );
    assert_eq!(first.duration_ticks, None, "no audioFile means no duration");
    assert_eq!(
        first.cover_path.as_deref(),
        Some("/api/items/show-2/cover"),
        "cover comes from media.coverPath"
    );
}

#[test]
fn null_episode_id_progress_is_skipped() {
    let json = r#"{"mediaProgress":[{"libraryItemId":"lib-1","episodeId":null,"currentTime":10.0,"isFinished":false}]}"#;
    let response: ProgressResponse = serde_json::from_str(json).unwrap();
    let mapped: HashMap<(String, String), AudiobookshelfProgress> = response
        .media_progress
        .into_iter()
        .filter_map(|x| {
            let episode_id = x.episode_id?;
            let value = AudiobookshelfProgress {
                library_item_id: x.library_item_id.clone(),
                episode_id: episode_id.clone(),
                current_time_seconds: x.current_time.unwrap_or(0.0).max(0.0),
                is_finished: x.is_finished.unwrap_or(false),
            };
            Some(((x.library_item_id, episode_id), value))
        })
        .collect();
    assert!(mapped.is_empty());
}

#[test]
fn covers_allow_present_and_missing_paths() {
    let cover_json = fixture("present-cover");
    let trimmed = cover_json.trim();
    let inner = trimmed
        .strip_prefix('{')
        .and_then(|s| s.strip_suffix('}'))
        .unwrap();
    let present: ShowWire = serde_json::from_str(&format!(
        "{{\"libraryItemId\":\"show-2\",\"title\":\"Show\",{inner}}}"
    ))
    .unwrap();
    assert_eq!(present.cover_path.as_deref(), Some("/cover/show-2"));
    let missing: serde_json::Value = serde_json::from_str(&fixture("missing-cover")).unwrap();
    assert!(missing["coverPath"].is_null());
}

#[test]
fn invalid_page_metadata_is_a_protocol_failure() {
    let page: ItemsResponse =
        serde_json::from_str(r#"{"page":0,"limit":20,"total":1,"results":[]}"#).unwrap();
    assert_eq!(page.page, 0);
}

#[test]
fn auth_failures_are_classified_and_errors_redact_credentials() {
    let error =
        AudiobookshelfError::new(super::super::AudiobookshelfFailureClass::AuthenticationRejected);
    assert!(!error.to_string().contains("secret-key"));
    assert!(serde_json::from_str::<ItemsResponse>("not json").is_err());
}

#[test]
fn surname_extraction_takes_last_token_and_falls_back_to_raw_credit() {
    assert_eq!(audiobook_author_sort_key("Tamora Pierce"), "Pierce");
    assert_eq!(
        audiobook_author_sort_key("Ursula K. Le Guin"),
        "Guin",
        "the final title-cased whitespace token is the sort surname"
    );
    assert_eq!(
        audiobook_author_sort_key(""),
        "",
        "empty credit falls back to the raw string"
    );
}

#[test]
fn multi_author_sort_uses_first_listed_surname_only() {
    assert_eq!(
        first_listed_author_sort_key("Sanderson, Brandon; Jordan, Robert"),
        "Sanderson"
    );
    assert_eq!(
        first_listed_author_sort_key("lee child"),
        "Child",
        "the surname is title-cased regardless of the credit's cashing"
    );
}

#[test]
fn author_display_prefers_authors_list_and_trims_single_author() {
    assert_eq!(
        book_author_display(
            None,
            Some(&[
                AuthorWire { name: "a".into() },
                AuthorWire { name: "b".into() }
            ])
        ),
        Some("a, b".into())
    );
    assert_eq!(
        book_author_display(Some(" Ferret "), None),
        Some("Ferret".into())
    );
    assert_eq!(book_author_display(Some("   "), None), None);
    assert_eq!(book_author_display(None, Some(&[])), None);
}

#[test]
fn book_list_page_parses_author_name_and_rich_metadata() {
    // Mirrors the live server's `/api/libraries/{id}/items` shape: the list
    // endpoint returns `authorName` (string), `narratorName`, `publishedYear`,
    // `genres`, `description`, and `media.duration` -- not the `author`/
    // `authors` fields the detail endpoint carries.
    let json = r#"{
        "page": 0, "limit": 1, "total": 1,
        "results": [{
            "id": "book-1",
            "media": {
                "duration": 48720.17,
                "coverPath": "/metadata/items/book-1/cover.jpg",
                "metadata": {
                    "title": "Other Rivers",
                    "authorName": "Peter Hessler",
                    "narratorName": "Peter Hessler",
                    "publishedYear": "2024",
                    "genres": ["Biographies & Memoirs", "Politics & Social Sciences"],
                    "description": "More than twenty years after teaching English..."
                }
            }
        }]
    }"#;
    let response: BooksResponse = serde_json::from_str(json).unwrap();
    assert_eq!(response.results.len(), 1);
    let wire = &response.results[0];
    let metadata = wire.media.as_ref().unwrap().metadata.as_ref().unwrap();
    assert_eq!(
        metadata.author.as_deref(),
        Some("Peter Hessler"),
        "authorName deserializes into the `author` field via the alias"
    );
    assert_eq!(metadata.narrator.as_deref(), Some("Peter Hessler"));
    assert_eq!(metadata.published_year.as_deref(), Some("2024"));
    assert_eq!(metadata.genres.as_ref().unwrap().len(), 2);
    assert!(metadata
        .description
        .as_deref()
        .unwrap()
        .contains("teaching"));
    assert_eq!(wire.media.as_ref().unwrap().duration, Some(48720.17));
}

#[test]
fn book_detail_page_parses_authors_object_list() {
    // The detail endpoint (`/api/items/{id}?expanded=1`) returns `authors` as
    // a list of `{id, name}` objects, not strings. The deserializer must
    // accept that shape and `book_author_display` must join the names.
    let json = r#"{
        "page": 0, "limit": 1, "total": 1,
        "results": [{
            "id": "book-1",
            "media": {
                "metadata": {
                    "title": "Co-authored Book",
                    "author": "First Author",
                    "authors": [
                        {"id": "a1", "name": "First Author"},
                        {"id": "a2", "name": "Second Author"}
                    ]
                }
            }
        }]
    }"#;
    let response: BooksResponse = serde_json::from_str(json).unwrap();
    let metadata = response.results[0]
        .media
        .as_ref()
        .unwrap()
        .metadata
        .as_ref()
        .unwrap();
    let display = book_author_display(metadata.author.as_deref(), metadata.authors.as_deref());
    assert_eq!(
        display.as_deref(),
        Some("First Author, Second Author"),
        "authors object list is joined for display, preferred over the single author string"
    );
}
