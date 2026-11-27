--- Personal records are held by exercise sets only

-- Warmup, cooldown and other steps lose their record flag; the records of their exercises are
-- recomputed among the exercise sets right after this script
UPDATE ACTIVITY_STEP SET pr = false WHERE kind <> 'Exercise';
