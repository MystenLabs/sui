CREATE TABLE network_metrics 
(
    checkpoint      BIGINT  PRIMARY KEY,
    epoch           BIGINT  NOT NULL,   
    timestamp_ms    BIGINT  NOT NULL,
    real_time_tps   FLOAT8  NOT NULL,
    peak_tps_30d    FLOAT8  NOT NULL, 
    total_addresses BIGINT  NOT NULL,
    total_objects   BIGINT  NOT NULL,
    total_packages  BIGINT  NOT NULL
);
