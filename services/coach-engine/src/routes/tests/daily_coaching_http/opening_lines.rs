use super::*;

#[tokio::test]
async fn find_is_a_public_catalog_read() {
    let application = application(Arc::new(FakeProfileValidator::default()));
    let request = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/opening-lines/find")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"query":"Najdorf"}"#))
        .unwrap();

    let response = application.oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let value: Value = serde_json::from_slice(&bytes).unwrap();

    assert_eq!(status, StatusCode::OK);
    assert_eq!(value["matches"].as_array().map(Vec::len), Some(10));
    assert_eq!(value["truncation"]["kind"], "truncated");
    assert!(
        value["truncation"]["totalMatchCount"]
            .as_u64()
            .expect("totalMatchCount")
            > 10
    );
    assert!(value["truncation"].get("oldestReturnedAt").is_none());
}

#[tokio::test]
async fn find_does_not_import_or_open_a_review_session() {
    let application = application(Arc::new(FakeProfileValidator::default()));
    let request = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/opening-lines/find")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"query":"C00"}"#))
        .unwrap();

    let response = application.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let imported = super::request(
        &application,
        Method::GET,
        "/api/v1/imported-games",
        Value::Null,
    )
    .await;
    assert_eq!(imported.0, StatusCode::OK);
    assert_eq!(imported.1["games"], json!([]));
}
