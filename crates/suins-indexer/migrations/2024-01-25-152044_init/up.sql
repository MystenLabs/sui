-- Your SQL goes here
CREATE TABLE "domains"(
	"name" VARCHAR NOT NULL PRIMARY KEY,
	"parent" VARCHAR NOT NULL,
	"expiration_timestamp_ms" INT8 NOT NULL,
	"nft_id" VARCHAR NOT NULL,
	"field_id" VARCHAR NOT NULL,
	"target_address" VARCHAR,
	"data" JSON NOT NULL,
	"last_checkpoint_updated" INT8 NOT NULL,
	"subdomain_wrapper_id" VARCHAR
);

