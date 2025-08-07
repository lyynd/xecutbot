CREATE TABLE IF NOT EXISTS visit (
    person INTEGER NOT NULL,
    day INTEGER NOT NULL,
    purpose TEXT NOT NULL,
    PRIMARY KEY (person, day)
);