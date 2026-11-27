//Auto generated file, do not edit manually

// From src-tauri/src/dto/activities.rs:107
export interface ActivityDetails {
  active_calories: number;
  active_time: number;
  connect_id: string | null;
  coordinates: ([number, number] | null)[];
  device: string | null;
  distance: number | null;
  feeling: number | null;
  heart_rates: (number | null)[];
  laps: ActivityLapDto[];
  max_hr: number;
  name: string;
  notes: string;
  primary_muscles: string[];
  rpe: number | null;
  secondary_muscles: string[];
  speeds: (number | null)[];
  sport: number;
  steps: ActivityStepDto[];
  sub_sport: number;
  timestamp: number;
  total_elapsed_time: number;
  training_load: number;
}

// From src-tauri/src/dto/activities.rs:90
export interface ActivityLapDto {
  idx: number;
  start_latitude: number | null;
  start_longitude: number | null;
}

// From src-tauri/src/dto/activities.rs:15
export interface ActivityListItem {
  active_calories: number;
  has_record: boolean;
  has_sets: boolean;
  name: string;
  sport: number;
  sub_sport: number;
  timestamp: number;
  total_elapsed_time: number;
  training_load: number;
}

// From src-tauri/src/dto/activities.rs:213
export interface ActivityLocation {
  activity: number;
  location: string;
}

// From src-tauri/src/dto/activities.rs:55
export interface ActivityStepDto {
  duration: number | null;
  duration_ref: number | null;
  ex_cat: number | null;
  ex_id: number | null;
  idx: number;
  kind: StepType;
  pr: boolean;
  reps: number | null;
  reps_ref: number | null;
  weight: number | null;
  weight_ref: number | null;
}

// From src-tauri/src/dto/activities.rs:199
export interface ActivityUpdate {
  notes: string | null;
  sets: ActivityStepDto[];
  timestamp: number;
}

// From src-tauri/src/dto/app.rs:10
export enum AppEnvironment {
	Debug = "Debug",
	Release = "Release",
}

// From src-tauri/src/dto/body_metrics.rs:12
export interface BodyMetricListItem {
  date: number;
  fat_ratio: number;
  muscle_mass: number;
  water_ratio: number;
  weight: number;
}

// From src-tauri/src/dto/devices.rs:6
export interface DeviceListItem {
  model: string;
  serial_number: string;
  strength_workout: boolean;
}

// From src-tauri/src/dao/settings.rs:104
export enum DistanceUnit {
	Kilometers = "Kilometers",
	Miles = "Miles",
}

// From src-tauri/src/dao/exercise.rs:30
export interface Exercise {
  category: number;
  dismissed_progression: number | null;
  exercise_category: ExerciseCategory | null;
  id: number;
  muscles: MuscleExercise[];
  name: string;
  notes: string | null;
  progression: boolean;
  reps: string;
  unilateral: boolean;
  weight: number;
}

// From src-tauri/src/dto/exercises.rs:38
export interface ExerciseActivity {
  series: ActivityStepDto[];
  weight: number | null;
}

// From src-tauri/src/dao/exercise_category.rs:9
export interface ExerciseCategory {
  id: number;
  name: string;
}

// From src-tauri/src/dto/exercises.rs:44
export interface ExerciseDetails {
  activities: Record<string, ExerciseActivity>;
  category: number;
  id: number;
  notes: string | null;
  pr_date: number;
  primary_muscles: string[];
  prog_weight: number;
  progression: boolean;
  reps: number;
  reps_range: string;
  secondary_muscles: string[];
  unilateral: boolean;
  weight: number;
  workouts: string[];
}

// From src-tauri/src/dto/exercises.rs:8
export interface ExerciseListItem {
  category: number;
  date: number;
  id: number;
  notes: string | null;
  progression: boolean;
  reps: number;
  unilateral: boolean;
  used: boolean;
  weight: number;
}

// From src-tauri/src/utils/translations.rs:10
export enum Languages {
	Spanish = "Spanish",
	English = "English",
}

// From src-tauri/src/dao/muscle.rs:5
export interface Muscle {
  name: string;
}

// From src-tauri/src/dao/muscle_exercise.rs:9
export interface MuscleExercise {
  ex_cat: number;
  ex_id: number;
  is_primary: boolean;
  muscle: string;
  muscle_obj: Muscle | null;
}

// From src-tauri/src/dto/activities.rs:207
export interface MuscleSets {
  primary: number;
  secondary: number;
}

// From src-tauri/src/dto/workouts.rs:55
export interface PercentLoadBase {
  ex_cat: number;
  ex_id: number;
  increment: number;
  one_rm: number;
}

// From src-tauri/src/dto/exercises.rs:103
export interface ProgressionSuggestion {
  reps: number;
  weight: number;
}

// From src-tauri/src/dto/prompts.rs:5
export enum PromptKind {
	ConnectCredentials = "ConnectCredentials",
	ConnectMfa = "ConnectMfa",
	SudoPassword = "SudoPassword",
}

// From src-tauri/src/dto/prompts.rs:17
export interface PromptRequest {
  detail: string | null;
  id: number;
  kind: PromptKind;
  prefill: string | null;
}

// From src-tauri/src/dto/report.rs:8
export interface ReportData {
  activities: ActivityDetails[];
  body_metrics: BodyMetricListItem[];
  exercises: ExerciseDetails[];
}

// From src-tauri/src/dto/app.rs:16
export interface Settings {
  distance_unit: DistanceUnit;
  language: Languages;
  on_device_connect: boolean;
  start_boot: boolean;
  weight_unit: WeightUnit;
}

// From src-tauri/src/dao/workout_step.rs:463
export enum StepType {
	Exercise = "Exercise",
	Warmup = "Warmup",
	Recovery = "Recovery",
	Cooldown = "Cooldown",
	Rest = "Rest",
	Other = "Other",
	Repeat = "Repeat",
}

// From src-tauri/src/dao/settings.rs:132
export enum WeightUnit {
	Kilograms = "Kilograms",
	Pounds = "Pounds",
}

// From src-tauri/src/dao/workout.rs:25
export interface Workout {
  connect_created_time: number | null;
  connect_id: string | null;
  enabled: boolean;
  id: number;
  name: string;
  notes: string | null;
  progression: boolean;
  steps: WorkoutStep[];
}

// From src-tauri/src/dto/workouts.rs:20
export interface WorkoutActivity {
  date: number;
  time: number;
  volume: number;
}

// From src-tauri/src/dto/workouts.rs:37
export interface WorkoutDetails {
  activities: WorkoutActivity[];
  activity_count: number;
  avg_time: number;
  avg_volume: number;
  enabled: boolean;
  id: number;
  latest_activity: number | null;
  name: string;
  notes: string | null;
  primary_muscles: string[];
  progression: boolean;
  secondary_muscles: string[];
  steps: WorkoutStep[];
}

// From src-tauri/src/dto/workouts.rs:8
export interface WorkoutListItem {
  activities: number;
  avg_load: number;
  avg_time: number;
  enabled: boolean;
  has_steps: boolean;
  id: number;
  latest_activity: number | null;
  name: string;
}

// From src-tauri/src/dto/workouts.rs:65
export interface WorkoutPlanListItem {
  activity: number | null;
  date: number;
  workout: number;
}

// From src-tauri/src/dao/workout_step.rs:36
export interface WorkoutStep {
  begin_idx: number | null;
  ex_cat: number | null;
  ex_id: number | null;
  exercise: Exercise | null;
  idx: number;
  kind: StepType;
  notes: string | null;
  one_rm_pct: number | null;
  reps: number | null | null;
  time: number | null | null;
  weight: number | null;
  workout: number;
}

