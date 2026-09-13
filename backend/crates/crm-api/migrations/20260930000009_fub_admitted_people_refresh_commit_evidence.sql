ALTER TABLE migration_admitted_people_refresh_item
  ADD COLUMN source_semantic_hmac BYTEA CHECK(source_semantic_hmac IS NULL OR octet_length(source_semantic_hmac)=32);
