--- Dismissed progression suggestions

-- Date of the exercise's latest activity when its suggestion was dismissed: the suggestion stays
-- hidden until a newer activity of the exercise
ALTER TABLE exercise ADD COLUMN dismissed_progression INTEGER;
