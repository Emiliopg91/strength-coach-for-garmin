--- Activity fields

ALTER TABLE activity ADD COLUMN feeling INTEGER;
ALTER TABLE activity ADD COLUMN rpe INTEGER;


ALTER TABLE ACTIVITY ADD COLUMN distance REAL;
ALTER TABLE ACTIVITY ADD COLUMN notes TEXT;
ALTER TABLE ACTIVITY ADD COLUMN max_hr INTEGER;

UPDATE ACTIVITY
SET distance = (SELECT d.distance FROM ACTIVITY_DATA d WHERE d.activity = ACTIVITY.date),
    notes    = (SELECT d.notes FROM ACTIVITY_DATA d WHERE d.activity = ACTIVITY.date),
    max_hr   = (SELECT d.max_hr FROM ACTIVITY_DATA d WHERE d.activity = ACTIVITY.date);

ALTER TABLE ACTIVITY_DATA DROP COLUMN distance;
ALTER TABLE ACTIVITY_DATA DROP COLUMN notes;
ALTER TABLE ACTIVITY_DATA DROP COLUMN max_hr;

-- Rows that only held those values have nothing left
DELETE FROM ACTIVITY_DATA WHERE heart_rates IS NULL AND coordinates IS NULL AND speeds IS NULL;

ALTER TABLE ACTIVITY ADD COLUMN metabolic_calories INTEGER;

-- Time the activity timer ran, without the pauses. Activities imported before it was kept can't
-- tell their pauses apart, so they take their elapsed time
ALTER TABLE ACTIVITY ADD COLUMN timer_time INTEGER NOT NULL DEFAULT 0;
UPDATE ACTIVITY SET timer_time = total_elapsed_time;
