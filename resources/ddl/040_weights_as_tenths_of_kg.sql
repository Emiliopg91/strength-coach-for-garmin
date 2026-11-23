--- Weights stored as integer tenths of a kg (12.5 kg -> 125) instead of REAL kg

-- SQLite can't change a column type. Rebuilding the tables would trip over the RESTRICT references
-- to EXERCISE (foreign keys stay on during migrations), so each column is replaced in place: a new
-- INTEGER column is filled from the old one, which is dropped, and the new one takes its name.
-- The record index covers ACTIVITY_STEP.weight, so it's dropped first and recreated at the end.
DROP INDEX ACTIVITY_STEP_EXERCISE_RECORD;

ALTER TABLE EXERCISE ADD COLUMN weight_new INTEGER NOT NULL DEFAULT 20;
UPDATE EXERCISE SET weight_new = CAST(ROUND(weight * 10) AS INTEGER);
ALTER TABLE EXERCISE DROP COLUMN weight;
ALTER TABLE EXERCISE RENAME COLUMN weight_new TO weight;

ALTER TABLE WORKOUT_STEP ADD COLUMN weight_new INTEGER;
UPDATE WORKOUT_STEP SET weight_new = CAST(ROUND(weight * 10) AS INTEGER) WHERE weight IS NOT NULL;
ALTER TABLE WORKOUT_STEP DROP COLUMN weight;
ALTER TABLE WORKOUT_STEP RENAME COLUMN weight_new TO weight;

ALTER TABLE ACTIVITY_STEP ADD COLUMN weight_new INTEGER;
ALTER TABLE ACTIVITY_STEP ADD COLUMN weight_ref_new INTEGER;
UPDATE ACTIVITY_STEP SET weight_new = CAST(ROUND(weight * 10) AS INTEGER) WHERE weight IS NOT NULL;
UPDATE ACTIVITY_STEP SET weight_ref_new = CAST(ROUND(weight_ref * 10) AS INTEGER) WHERE weight_ref IS NOT NULL;
ALTER TABLE ACTIVITY_STEP DROP COLUMN weight;
ALTER TABLE ACTIVITY_STEP DROP COLUMN weight_ref;
ALTER TABLE ACTIVITY_STEP RENAME COLUMN weight_new TO weight;
ALTER TABLE ACTIVITY_STEP RENAME COLUMN weight_ref_new TO weight_ref;

ALTER TABLE BODY_METRIC ADD COLUMN weight_new INTEGER NOT NULL DEFAULT 0;
ALTER TABLE BODY_METRIC ADD COLUMN muscle_mass_new INTEGER NOT NULL DEFAULT 0;
UPDATE BODY_METRIC SET weight_new = CAST(ROUND(weight * 10) AS INTEGER),
                       muscle_mass_new = CAST(ROUND(muscle_mass * 10) AS INTEGER);
ALTER TABLE BODY_METRIC DROP COLUMN weight;
ALTER TABLE BODY_METRIC DROP COLUMN muscle_mass;
ALTER TABLE BODY_METRIC RENAME COLUMN weight_new TO weight;
ALTER TABLE BODY_METRIC RENAME COLUMN muscle_mass_new TO muscle_mass;

CREATE INDEX ACTIVITY_STEP_EXERCISE_RECORD ON ACTIVITY_STEP(exercise_category, exercise_id, weight DESC, reps DESC, activity, idx);
