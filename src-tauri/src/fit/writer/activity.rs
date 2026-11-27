use chrono::{Local, TimeZone};
use rustyfit::{
    profile::{
        mesgdef::{
            Activity as FitActivity, DeviceInfo, Event, FileId, Lap, Record, Session as FitSession,
            Set, Sport as FitSport, TimeInZone, TrainingFile, Workout as FitWorkout,
        },
        typedef::{
            self, DateTime, DeviceIndex, EventType, ExerciseCategory, FitBaseUnit, LapTrigger,
            LocalDateTime, MesgNum, MessageIndex, SessionTrigger, SetType, Sport, SubSport,
        },
    },
    proto::Message,
};

use crate::utils::weight::to_kg;
use crate::{
    dao::{
        activity::Activity,
        activity_data::ActivityData,
        activity_step::ActivityStep,
        device::Device,
        settings::WeightUnit,
        workout::Workout,
        workout_step::{StepType, WorkoutStep},
    },
    fit::writer::{FitWriter, ToFitMessages, errors},
    utils::translations::{Languages, translate},
};

fn datetime(secs: u32) -> DateTime {
    DateTime::from_unix_timestamp(secs as i64)
}

/// Point `part / total` of the way through `elapsed` seconds after `start`.
fn time_at(start: u32, elapsed: u32, part: usize, total: usize) -> u32 {
    start + (part as u64 * elapsed as u64 / total as u64) as u32
}

fn timer_event(timestamp: u32, event_type: EventType) -> Message {
    let mut event = Event::new();
    event.timestamp = datetime(timestamp);
    event.event = typedef::Event::TIMER;
    event.event_type = event_type;
    event.into()
}

/// Activities don't keep record times, so samples are spread evenly as the charts assume.
fn record_messages(data: &ActivityData, start: u32, elapsed: u32) -> Vec<Message> {
    let hrs = data.get_heart_rates().unwrap_or_default();
    let coords = data.get_coordinates_semicircle().unwrap_or_default();
    let speeds = data.get_speeds().unwrap_or_default();
    let count = hrs.len().max(coords.len()).max(speeds.len());

    (0..count)
        .map(|i| {
            let mut record = Record::new();
            record.timestamp = datetime(time_at(start, elapsed, i, count));
            if let Some(Some(hr)) = hrs.get(i) {
                record.heart_rate = *hr;
            }
            if let Some(Some((lat, long))) = coords.get(i) {
                record.position_lat = *lat;
                record.position_long = *long;
            }
            if let Some(Some(speed)) = speeds.get(i) {
                // The scaled setter truncates, which would shift the value on every round trip
                record.enhanced_speed = (speed * 1000.0).round() as u32;
            }
            record.into()
        })
        .collect()
}

/// One step per set, as the sets don't keep which step of the original workout they came from
/// (and that workout may have changed since). The parser reads the targets back from these steps.
fn workout_step(workout: u16, idx: u16, step: &ActivityStep) -> WorkoutStep {
    let mut wkt_step = WorkoutStep::new(workout, idx, step.kind.clone());
    wkt_step.ex_cat = step.ex_cat;
    wkt_step.ex_id = step.ex_id;
    wkt_step.weight = step.weight_ref;
    // A step has a single duration, and stored sets may have both targets: reps are the real one
    match step.reps_ref.filter(|reps| *reps > 0) {
        Some(reps) => wkt_step.reps = Some(Some(reps)),
        None => wkt_step.time = Some(step.duration_ref),
    }
    wkt_step
}

fn sport_name(activity: &Activity, lang: Languages) -> String {
    translate(
        &format!("subsport_{}_{}", activity.sport, activity.sub_sport),
        lang,
    )
}

fn sport_message(activity: &Activity, lang: Languages) -> Message {
    let mut sport = FitSport::new();
    sport.sport = Sport(activity.sport);
    sport.sub_sport = SubSport(activity.sub_sport);
    sport.name = sport_name(activity, lang).into();
    sport.into()
}

/// Reference to the workout file followed in the activity, which devices write next to the workout.
/// It carries the `file_id` keys of the workout file, so it points to the same file the device has.
/// Without a Connect creation time the workout file is stamped when written, which can't be known
/// here, so the activity date stands in.
fn training_file_message(activity: &Activity, workout: &Workout) -> Message {
    let file_id = FileId::from(&FitWriter::from(workout).file_id());
    let mut training_file = TrainingFile::new();
    training_file.timestamp = datetime(activity.date);
    training_file.r#type = file_id.r#type;
    training_file.manufacturer = file_id.manufacturer;
    training_file.product = file_id.product;
    training_file.serial_number = file_id.serial_number;
    training_file.time_created = if workout.file_created().is_some() {
        file_id.time_created
    } else {
        datetime(activity.date)
    };
    training_file.into()
}

fn set_message(idx: u16, step: &ActivityStep, start: u32, weight_unit: WeightUnit) -> Message {
    let mut set = Set::new();
    set.message_index = MessageIndex(idx);
    set.wkt_step_index = MessageIndex(idx);
    set.start_time = datetime(start);
    set.timestamp = datetime(start + step.duration.unwrap_or(0));
    if let Some(duration) = step.duration {
        set.set_duration_scaled(duration as f64);
    }
    set.set_type = if step.kind == StepType::Rest {
        SetType::REST
    } else {
        SetType::ACTIVE
    };

    if let (Some(cat), Some(id)) = (step.ex_cat, step.ex_id) {
        set.category = vec![ExerciseCategory(cat)].into();
        set.category_subtype = vec![id].into();
        set.repetitions = step.reps.unwrap_or(0);
        set.set_weight_scaled(f64::from(to_kg(step.weight.unwrap_or(0))));
        set.weight_display_unit = match weight_unit {
            WeightUnit::Kilograms => FitBaseUnit::KILOGRAM,
            WeightUnit::Pounds => FitBaseUnit::POUND,
        };
    }

    set.into()
}

fn activity_end(activity: &Activity) -> u32 {
    activity.date + activity.total_elapsed_time
}

fn unit_id(activity: &Activity) -> Option<u32> {
    let unit_id = Device::unit_id_from_serial(activity.device.as_deref()?).ok()?;
    u32::try_from(unit_id).ok()
}

fn first_position(activity: &Activity) -> Option<(i32, i32)> {
    activity
        .data
        .as_ref()?
        .get_coordinates_semicircle()?
        .into_iter()
        .flatten()
        .next()
}

fn sorted_steps(activity: &Activity) -> Vec<&ActivityStep> {
    let mut steps: Vec<_> = activity.steps.iter().collect();
    steps.sort_by_key(|s| s.idx);
    steps
}

/// The workout message carries the activity name, even for activities without sets.
fn workout_messages(
    activity: &Activity,
    lang: Languages,
    weight_unit: WeightUnit,
) -> errors::Result<Vec<Message>> {
    if activity.steps.is_empty() && activity.name.is_empty() {
        return Ok(vec![]);
    }

    let workout_id = activity.workout.unwrap_or_default();
    let workout = Workout {
        id: workout_id,
        name: activity.name.clone(),
        enabled: true,
        notes: activity.workout_obj.as_ref().and_then(|w| w.notes.clone()),
        connect_id: activity
            .workout_obj
            .as_ref()
            .and_then(|w| w.connect_id.clone()),
        connect_created_time: activity
            .workout_obj
            .as_ref()
            .and_then(|w| w.connect_created_time),
        steps: sorted_steps(activity)
            .into_iter()
            .enumerate()
            .map(|(idx, step)| workout_step(workout_id, idx as u16, step))
            .collect(),
        progression: true,
    };

    let mut messages = workout.to_fit_messages(lang, weight_unit)?;
    let mut fit_workout = FitWorkout::from(&messages[0]);
    fit_workout.sport = Sport(activity.sport);
    fit_workout.sub_sport = SubSport(activity.sub_sport);
    messages[0] = fit_workout.into();
    messages.insert(0, training_file_message(activity, &workout));

    Ok(messages)
}

/// Sets don't keep their start time, so they are laid out back to back from the activity start.
fn set_messages(activity: &Activity, weight_unit: WeightUnit) -> Vec<Message> {
    let mut start = activity.date;
    sorted_steps(activity)
        .into_iter()
        .enumerate()
        .map(|(idx, step)| {
            let msg = set_message(idx as u16, step, start, weight_unit);
            start += step.duration.unwrap_or(0);
            msg
        })
        .collect()
}

/// The parser drops the first lap, so it is rebuilt here. Laps don't keep their times, so the
/// activity is split evenly among them.
fn lap_messages(activity: &Activity) -> Vec<Message> {
    let mut laps: Vec<_> = activity.laps.iter().collect();
    laps.sort_by_key(|l| l.idx);

    let first_position = first_position(activity);
    let starts = std::iter::once((first_position.map(|p| p.0), first_position.map(|p| p.1)))
        .chain(laps.iter().map(|l| (l.start_latitude, l.start_longitude)));
    let count = laps.len() + 1;

    starts
        .enumerate()
        .map(|(idx, (lat, long))| {
            let start = time_at(activity.date, activity.total_elapsed_time, idx, count);
            let end = time_at(activity.date, activity.total_elapsed_time, idx + 1, count);

            let mut lap = Lap::new();
            lap.message_index = MessageIndex(idx as u16);
            lap.event = typedef::Event::LAP;
            lap.event_type = EventType::STOP;
            lap.lap_trigger = if idx + 1 == count {
                LapTrigger::SESSION_END
            } else {
                LapTrigger::MANUAL
            };
            lap.sport = Sport(activity.sport);
            lap.start_time = datetime(start);
            lap.timestamp = datetime(end);
            // The pauses aren't kept, so the timer time is spread the same way as the elapsed one
            let timer = time_at(0, activity.timer_time, idx + 1, count)
                - time_at(0, activity.timer_time, idx, count);

            lap.set_total_elapsed_time_scaled((end - start) as f64);
            lap.set_total_timer_time_scaled(timer as f64);
            if let (Some(lat), Some(long)) = (lat, long) {
                lap.start_position_lat = lat;
                lap.start_position_long = long;
            }
            lap.into()
        })
        .collect()
}

fn session_message(activity: &Activity, num_laps: u16, lang: Languages) -> Message {
    let mut msg = FitSession::new();
    msg.message_index = MessageIndex(0);
    // The activity date is its start, which is what devices record in this timestamp
    msg.timestamp = datetime(activity.date);
    msg.start_time = datetime(activity.date);
    msg.event = typedef::Event::SESSION;
    msg.event_type = EventType::STOP;
    msg.trigger = SessionTrigger::ACTIVITY_END;
    msg.sport = Sport(activity.sport);
    msg.sub_sport = SubSport(activity.sub_sport);
    msg.sport_profile_name = sport_name(activity, lang).into();
    msg.first_lap_index = 0;
    msg.num_laps = num_laps;
    msg.set_total_elapsed_time_scaled(activity.total_elapsed_time as f64);
    msg.set_total_timer_time_scaled(activity.timer_time as f64);
    msg.set_active_time_scaled(activity.active_time as f64);
    // Scaled by 65536: the scaled setter truncates, which would shift the value on every round trip
    msg.training_load_peak = (f64::from(activity.training_load) * 65536.0).round() as i32;
    // Activities imported before metabolic calories were kept only have the active ones
    let metabolic_calories = activity.metabolic_calories.unwrap_or_default();
    msg.total_calories = activity.active_calories.saturating_add(metabolic_calories);
    msg.metabolic_calories = metabolic_calories;
    msg.workout_feel = activity.feeling.map_or(u8::MAX, |f| f * 25);
    msg.workout_rpe = activity.rpe.map_or(u8::MAX, |f| f * 10);

    if let Some((lat, long)) = first_position(activity) {
        msg.start_position_lat = lat;
        msg.start_position_long = long;
    }

    if let Some(distance) = activity.distance {
        // The scaled setter truncates, which would shift the value on every round trip
        msg.total_distance = (distance * 100.0).round() as u32;
    }

    if let Some(data) = &activity.data {
        let hrs: Vec<u32> = data
            .get_heart_rates()
            .unwrap_or_default()
            .into_iter()
            .flatten()
            .filter(|hr| *hr > 0)
            .map(u32::from)
            .collect();
        if let Some(max) = hrs.iter().max() {
            msg.max_heart_rate = *max as u8;
            msg.avg_heart_rate = (hrs.iter().sum::<u32>() / hrs.len() as u32) as u8;
        }
    }

    msg.into()
}

fn activity_message(activity: &Activity) -> Message {
    let end = activity_end(activity);
    let offset = Local
        .timestamp_opt(end as i64, 0)
        .single()
        .map_or(0, |d| d.offset().local_minus_utc());

    let mut fit_activity = FitActivity::new();
    fit_activity.timestamp = datetime(end);
    fit_activity.local_timestamp = LocalDateTime::from_unix_timestamp(end as i64 + offset as i64);
    fit_activity.set_total_timer_time_scaled(activity.timer_time as f64);
    fit_activity.num_sessions = 1;
    fit_activity.r#type = typedef::Activity::MANUAL;
    fit_activity.event = typedef::Event::ACTIVITY;
    fit_activity.event_type = EventType::STOP;
    fit_activity.into()
}

impl ToFitMessages for Activity {
    fn file_type(&self) -> typedef::File {
        typedef::File::ACTIVITY
    }

    fn file_key(&self) -> u32 {
        // Activities coming from a file may not have a device; the date is unique among activities anyway
        unit_id(self).unwrap_or(self.date)
    }

    /// Inverse of the mapping done in `TryFrom<FitParser> for Activity`.
    fn to_fit_messages(
        &self,
        lang: Languages,
        weight_unit: WeightUnit,
    ) -> errors::Result<Vec<Message>> {
        let mut messages = Vec::new();

        if let Some(unit_id) = unit_id(self) {
            let mut device = DeviceInfo::new();
            device.timestamp = datetime(self.date);
            device.device_index = DeviceIndex::CREATOR;
            device.serial_number = unit_id;
            messages.push(device.into());
        }

        messages.push(timer_event(self.date, EventType::START));
        messages.push(sport_message(self, lang));
        messages.extend(workout_messages(self, lang, weight_unit)?);

        if let Some(data) = &self.data {
            messages.extend(record_messages(data, self.date, self.total_elapsed_time));
        }

        if let Some(max_hr) = self.max_hr {
            let mut zones = TimeInZone::new();
            zones.timestamp = datetime(activity_end(self));
            zones.reference_mesg = MesgNum::SESSION;
            zones.reference_index = MessageIndex(0);
            zones.max_heart_rate = max_hr;
            messages.push(zones.into());
        }

        messages.extend(set_messages(self, weight_unit));

        let laps = lap_messages(self);
        let num_laps = laps.len() as u16;
        messages.extend(laps);

        messages.push(timer_event(activity_end(self), EventType::STOP_ALL));
        messages.push(session_message(self, num_laps, lang));
        messages.push(activity_message(self));

        Ok(messages)
    }
}
