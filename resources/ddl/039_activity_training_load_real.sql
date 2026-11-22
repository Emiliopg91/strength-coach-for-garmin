--- Activity training load keeps its decimals instead of being rounded to an integer

-- SQLite can't change a column type, so ACTIVITY is rebuilt. Migrations run with foreign keys on:
-- dropping it would cascade to the tables referencing it, and renaming it would move their
-- references along, so those tables are rebuilt with it from copies
CREATE TABLE MIG_ACTIVITY AS SELECT * FROM ACTIVITY;
CREATE TABLE MIG_ACTIVITY_DATA AS SELECT * FROM ACTIVITY_DATA;
CREATE TABLE MIG_ACTIVITY_LAP AS SELECT * FROM ACTIVITY_LAP;
CREATE TABLE MIG_ACTIVITY_STEP AS SELECT * FROM ACTIVITY_STEP;
CREATE TABLE MIG_WORKOUT_PLANNING AS SELECT * FROM WORKOUT_PLANNING;

DROP TABLE WORKOUT_PLANNING;
DROP TABLE ACTIVITY_STEP;
DROP TABLE ACTIVITY_LAP;
DROP TABLE ACTIVITY_DATA;
DROP TABLE ACTIVITY;

CREATE TABLE ACTIVITY (
    date               INTEGER NOT NULL PRIMARY KEY,
    name               TEXT NOT NULL,
    total_elapsed_time INTEGER NOT NULL,
    active_time        INTEGER NOT NULL DEFAULT 0,
    active_calories    INTEGER NOT NULL DEFAULT 0,
    training_load      REAL NOT NULL DEFAULT 0,
    device             TEXT,
    sport              INTEGER NOT NULL,
    sub_sport          INTEGER NOT NULL,
    connect_id         TEXT DEFAULT NULL,
    workout            INTEGER REFERENCES WORKOUT(id) ON DELETE SET NULL,
    FOREIGN KEY (sport, sub_sport) REFERENCES SUB_SPORT(sport, id) ON DELETE RESTRICT
);

CREATE TABLE ACTIVITY_DATA (
    activity    INTEGER NOT NULL PRIMARY KEY,
    heart_rates BLOB,
    coordinates BLOB,
    speeds      BLOB,
    distance    REAL,
    notes       TEXT,
    max_hr      INTEGER,
    FOREIGN KEY (activity) REFERENCES ACTIVITY(date) ON DELETE CASCADE
);

CREATE TABLE ACTIVITY_LAP (
    activity        INTEGER NOT NULL,
    idx             INTEGER NOT NULL,
    start_latitude  INTEGER,
    start_longitude INTEGER,
    PRIMARY KEY (activity, idx),
    FOREIGN KEY (activity) REFERENCES ACTIVITY(date) ON DELETE CASCADE
);

CREATE TABLE ACTIVITY_STEP (
    activity          INTEGER NOT NULL,
    idx               INTEGER NOT NULL,
    kind              TEXT NOT NULL,
    duration          INTEGER,
    duration_ref      INTEGER,
    exercise_category INTEGER,
    exercise_id       INTEGER,
    reps              INTEGER,
    reps_ref          INTEGER,
    weight            REAL,
    weight_ref        REAL,
    pr                BOOLEAN NOT NULL,
    PRIMARY KEY (activity, idx),
    FOREIGN KEY (activity) REFERENCES ACTIVITY(date) ON DELETE CASCADE,
    FOREIGN KEY (exercise_category, exercise_id) REFERENCES EXERCISE(category, id) ON DELETE RESTRICT
);

CREATE TABLE WORKOUT_PLANNING (
    date       INTEGER NOT NULL,
    workout    INTEGER NOT NULL,
    activity   INTEGER,
    connect_id TEXT,
    PRIMARY KEY (date, workout),
    FOREIGN KEY (workout) REFERENCES WORKOUT(id) ON DELETE CASCADE,
    FOREIGN KEY (activity) REFERENCES ACTIVITY(date) ON DELETE SET NULL
);

INSERT INTO ACTIVITY (date, name, total_elapsed_time, active_time, active_calories, training_load,
                      device, sport, sub_sport, connect_id, workout)
SELECT date, name, total_elapsed_time, active_time, active_calories, training_load,
       device, sport, sub_sport, connect_id, workout
FROM MIG_ACTIVITY;

INSERT INTO ACTIVITY_DATA (activity, heart_rates, coordinates, speeds, distance, notes, max_hr)
SELECT activity, heart_rates, coordinates, speeds, distance, notes, max_hr FROM MIG_ACTIVITY_DATA;

INSERT INTO ACTIVITY_LAP (activity, idx, start_latitude, start_longitude)
SELECT activity, idx, start_latitude, start_longitude FROM MIG_ACTIVITY_LAP;

INSERT INTO ACTIVITY_STEP (activity, idx, kind, duration, duration_ref, exercise_category,
                           exercise_id, reps, reps_ref, weight, weight_ref, pr)
SELECT activity, idx, kind, duration, duration_ref, exercise_category,
       exercise_id, reps, reps_ref, weight, weight_ref, pr
FROM MIG_ACTIVITY_STEP;

INSERT INTO WORKOUT_PLANNING (date, workout, activity, connect_id)
SELECT date, workout, activity, connect_id FROM MIG_WORKOUT_PLANNING;

DROP TABLE MIG_WORKOUT_PLANNING;
DROP TABLE MIG_ACTIVITY_STEP;
DROP TABLE MIG_ACTIVITY_LAP;
DROP TABLE MIG_ACTIVITY_DATA;
DROP TABLE MIG_ACTIVITY;

CREATE INDEX ACTIVITY_NAME ON ACTIVITY(name);
CREATE INDEX ACTIVITY_WORKOUT ON ACTIVITY(workout);
CREATE INDEX ACTIVITY_STEP_ACTIVITY_PR ON ACTIVITY_STEP(activity, pr);
CREATE UNIQUE INDEX ACTIVITY_STEP_EXERCISE_PR ON ACTIVITY_STEP(exercise_category, exercise_id) WHERE pr = true;
CREATE INDEX ACTIVITY_STEP_EXERCISE_RECORD ON ACTIVITY_STEP(exercise_category, exercise_id, weight DESC, reps DESC, activity, idx);
CREATE INDEX WORKOUT_PLANNING_ACTIVITY ON WORKOUT_PLANNING(activity);
