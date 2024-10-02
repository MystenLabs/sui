alter table progress_store
    add target_checkpoint BIGINT default 9223372036854775807 not null;

alter table progress_store
    add timestamp timestamp default now() not null;