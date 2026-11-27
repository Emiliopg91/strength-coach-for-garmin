//Auto generated file, do not edit manually

import { invoke, InvokeArgs } from "@tauri-apps/api/core";

import { ActivityDetails, ActivityListItem, ActivityUpdate, AppEnvironment, BodyMetricListItem, DeviceListItem, ExerciseDetails, ExerciseListItem, MuscleSets, PercentLoadBase, ProgressionSuggestion, ReportData, Settings, Workout, WorkoutDetails, WorkoutListItem, WorkoutPlanListItem } from "./models";

export class BackendClient {

    private static DONT_LOG_COMMANDS: string[] = ["answer_prompt"];

	// From src-tauri/src/logic/body_metrics/commands.rs:22
	public static addBodyMeasures(measures: BodyMetricListItem): Promise<void> {
	  return BackendClient.inner_invoke("add_body_measures", { measures }); 
	}
	

	// From src-tauri/src/logic/workouts/commands.rs:150
	public static addPlannedWorkout(date: number, workout: number): Promise<void> {
	  return BackendClient.inner_invoke("add_planned_workout", { date, workout }); 
	}
	

	// From src-tauri/src/logic/prompts/commands.rs:13
	public static answerPrompt(id: number, values: string[] | null): Promise<void> {
	  return BackendClient.inner_invoke("answer_prompt", { id, values }); 
	}
	

	// From src-tauri/src/logic/exercises/commands.rs:146
	public static applyProgressionSuggestion(category: number, id: number, suggestion: ProgressionSuggestion): Promise<void> {
	  return BackendClient.inner_invoke("apply_progression_suggestion", { category, id, suggestion }); 
	}
	

	// From src-tauri/src/logic/exercises/commands.rs:134
	public static availableAnyProgressionSuggestions(): Promise<boolean> {
	  return BackendClient.inner_invoke("available_any_progression_suggestions"); 
	}
	

	// From src-tauri/src/logic/connect/commands.rs:43
	public static connectLoggedUser(): Promise<string | null> {
	  return BackendClient.inner_invoke("connect_logged_user"); 
	}
	

	// From src-tauri/src/logic/connect/commands.rs:54
	public static connectLogin(): Promise<string | null> {
	  return BackendClient.inner_invoke("connect_login"); 
	}
	

	// From src-tauri/src/logic/connect/commands.rs:66
	public static connectLogout(): Promise<void> {
	  return BackendClient.inner_invoke("connect_logout"); 
	}
	

	// From src-tauri/src/logic/body_metrics/commands.rs:34
	public static deleteBodyMetric(date: number): Promise<void> {
	  return BackendClient.inner_invoke("delete_body_metric", { date }); 
	}
	

	// From src-tauri/src/logic/workouts/commands.rs:136
	public static deletePlannedWorkout(date: number, workout: number): Promise<void> {
	  return BackendClient.inner_invoke("delete_planned_workout", { date, workout }); 
	}
	

	// From src-tauri/src/logic/exercises/commands.rs:166
	public static dismissProgressionSuggestion(category: number, id: number): Promise<void> {
	  return BackendClient.inner_invoke("dismiss_progression_suggestion", { category, id }); 
	}
	

	// From src-tauri/src/logic/activities/commands.rs:51
	public static downloadActivity(timestamp: number): Promise<void> {
	  return BackendClient.inner_invoke("download_activity", { timestamp }); 
	}
	

	// From src-tauri/src/logic/activities/commands.rs:23
	public static getActivities(limit: number | null): Promise<ActivityListItem[]> {
	  return BackendClient.inner_invoke("get_activities", { limit }); 
	}
	

	// From src-tauri/src/logic/activities/commands.rs:36
	public static getActivityDetails(timestamp: number): Promise<ActivityDetails> {
	  return BackendClient.inner_invoke("get_activity_details", { timestamp }); 
	}
	

	// From src-tauri/src/logic/body_metrics/commands.rs:11
	public static getBodyMeasures(): Promise<BodyMetricListItem[]> {
	  return BackendClient.inner_invoke("get_body_measures"); 
	}
	

	// From src-tauri/src/logic/app/commands.rs:71
	public static getEnvironment(): Promise<AppEnvironment> {
	  return BackendClient.inner_invoke("get_environment"); 
	}
	

	// From src-tauri/src/logic/exercises/commands.rs:29
	public static getExerciseDetails(category: number, id: number): Promise<ExerciseDetails> {
	  return BackendClient.inner_invoke("get_exercise_details", { category, id }); 
	}
	

	// From src-tauri/src/logic/exercises/commands.rs:18
	public static getExercises(): Promise<ExerciseListItem[]> {
	  return BackendClient.inner_invoke("get_exercises"); 
	}
	

	// From src-tauri/src/logic/exercises/commands.rs:42
	public static getExercisesCatalog(): Promise<Record<number, number[]>> {
	  return BackendClient.inner_invoke("get_exercises_catalog"); 
	}
	

	// From src-tauri/src/logic/activities/commands.rs:152
	public static getHeatmapData(): Promise<[number, number][][]> {
	  return BackendClient.inner_invoke("get_heatmap_data"); 
	}
	

	// From src-tauri/src/logic/activities/commands.rs:163
	public static getMuscleSets(since: number): Promise<Record<string, MuscleSets>> {
	  return BackendClient.inner_invoke("get_muscle_sets", { since }); 
	}
	

	// From src-tauri/src/logic/workouts/commands.rs:164
	public static getNextPlannedWorkout(): Promise<WorkoutPlanListItem | null> {
	  return BackendClient.inner_invoke("get_next_planned_workout"); 
	}
	

	// From src-tauri/src/logic/workouts/commands.rs:117
	public static getPercentLoadBases(): Promise<PercentLoadBase[]> {
	  return BackendClient.inner_invoke("get_percent_load_bases"); 
	}
	

	// From src-tauri/src/logic/workouts/commands.rs:127
	public static getPlannedWorkouts(): Promise<WorkoutPlanListItem[]> {
	  return BackendClient.inner_invoke("get_planned_workouts"); 
	}
	

	// From src-tauri/src/logic/exercises/commands.rs:112
	public static getProgressionStatus(category: number, id: number): Promise<ProgressionSuggestion | null> {
	  return BackendClient.inner_invoke("get_progression_status", { category, id }); 
	}
	

	// From src-tauri/src/logic/exercises/commands.rs:123
	public static getProgressionSuggestion(category: number, id: number): Promise<ProgressionSuggestion | null> {
	  return BackendClient.inner_invoke("get_progression_suggestion", { category, id }); 
	}
	

	// From src-tauri/src/logic/devices/commands.rs:48
	public static getRegisteredDevices(): Promise<DeviceListItem[]> {
	  return BackendClient.inner_invoke("get_registered_devices"); 
	}
	

	// From src-tauri/src/logic/report/commands.rs:42
	public static getReportData(from: number, to: number): Promise<ReportData> {
	  return BackendClient.inner_invoke("get_report_data", { from, to }); 
	}
	

	// From src-tauri/src/logic/app/commands.rs:22
	public static getSettings(): Promise<Settings> {
	  return BackendClient.inner_invoke("get_settings"); 
	}
	

	// From src-tauri/src/logic/app/commands.rs:113
	public static getTranslations(): Promise<Record<string, string>> {
	  return BackendClient.inner_invoke("get_translations"); 
	}
	

	// From src-tauri/src/logic/workouts/commands.rs:31
	public static getWorkoutDetails(id: number): Promise<WorkoutDetails> {
	  return BackendClient.inner_invoke("get_workout_details", { id }); 
	}
	

	// From src-tauri/src/logic/workouts/commands.rs:20
	public static getWorkoutList(): Promise<WorkoutListItem[]> {
	  return BackendClient.inner_invoke("get_workout_list"); 
	}
	

	// From src-tauri/src/logic/connect/commands.rs:16
	public static importFromConnect(): Promise<number> {
	  return BackendClient.inner_invoke("import_from_connect"); 
	}
	

	// From src-tauri/src/logic/devices/commands.rs:33
	public static importFromDevice(serial: string): Promise<number> {
	  return BackendClient.inner_invoke("import_from_device", { serial }); 
	}
	

	// From src-tauri/src/logic/activities/commands.rs:118
	public static importFromFiles(): Promise<number> {
	  return BackendClient.inner_invoke("import_from_files"); 
	}
	

	// From src-tauri/src/logic/connect/commands.rs:32
	public static importWeightFromConnect(): Promise<number> {
	  return BackendClient.inner_invoke("import_weight_from_connect"); 
	}
	

	// From src-tauri/src/logic/report/commands.rs:20
	public static loadReport(from: number, to: number): Promise<void> {
	  return BackendClient.inner_invoke("load_report", { from, to }); 
	}
	

	// From src-tauri/src/logic/workouts/commands.rs:96
	public static loadWorkoutPage(workout: number): Promise<void> {
	  return BackendClient.inner_invoke("load_workout_page", { workout }); 
	}
	

	// From src-tauri/src/logic/app/commands.rs:31
	public static notifyFrontendReady(): Promise<void> {
	  return BackendClient.inner_invoke("notify_frontend_ready"); 
	}
	

	// From src-tauri/src/logic/report/commands.rs:31
	public static reportReady(): Promise<void> {
	  return BackendClient.inner_invoke("report_ready"); 
	}
	

	// From src-tauri/src/logic/activities/commands.rs:71
	public static saveActivityChanges(details: ActivityUpdate): Promise<void> {
	  return BackendClient.inner_invoke("save_activity_changes", { details }); 
	}
	

	// From src-tauri/src/logic/exercises/commands.rs:179
	public static saveExerciseNotes(category: number, id: number, notes: string): Promise<void> {
	  return BackendClient.inner_invoke("save_exercise_notes", { category, id, notes }); 
	}
	

	// From src-tauri/src/logic/workouts/commands.rs:81
	public static saveWorkout(edit: boolean, workout: Workout): Promise<void> {
	  return BackendClient.inner_invoke("save_workout", { edit, workout }); 
	}
	

	// From src-tauri/src/logic/connect/commands.rs:75
	public static sendToConnect(workout: number, serial: string): Promise<void> {
	  return BackendClient.inner_invoke("send_to_connect", { workout, serial }); 
	}
	

	// From src-tauri/src/logic/devices/commands.rs:17
	public static sendToDevice(workout: number, serial: string): Promise<void> {
	  return BackendClient.inner_invoke("send_to_device", { workout, serial }); 
	}
	

	// From src-tauri/src/logic/activities/commands.rs:102
	public static setActivityFeeling(timestamp: number, feeling: number | null): Promise<void> {
	  return BackendClient.inner_invoke("set_activity_feeling", { timestamp, feeling }); 
	}
	

	// From src-tauri/src/logic/activities/commands.rs:88
	public static setActivityRpe(timestamp: number, rpe: number | null): Promise<void> {
	  return BackendClient.inner_invoke("set_activity_rpe", { timestamp, rpe }); 
	}
	

	// From src-tauri/src/logic/exercises/commands.rs:69
	public static setExerciseProgression(category: number, id: number, value: boolean): Promise<void> {
	  return BackendClient.inner_invoke("set_exercise_progression", { category, id, value }); 
	}
	

	// From src-tauri/src/logic/exercises/commands.rs:87
	public static setExerciseProgressionParameters(category: number, id: number, reps: string, weight: number): Promise<void> {
	  return BackendClient.inner_invoke("set_exercise_progression_parameters", { category, id, reps, weight }); 
	}
	

	// From src-tauri/src/logic/exercises/commands.rs:53
	public static setExerciseUnilateral(category: number, id: number, value: boolean): Promise<void> {
	  return BackendClient.inner_invoke("set_exercise_unilateral", { category, id, value }); 
	}
	

	// From src-tauri/src/logic/workouts/commands.rs:60
	public static setWorkoutProgression(workout: number, value: boolean): Promise<void> {
	  return BackendClient.inner_invoke("set_workout_progression", { workout, value }); 
	}
	

	// From src-tauri/src/logic/workouts/commands.rs:43
	public static setWorkoutStatus(workout: number, status: boolean): Promise<void> {
	  return BackendClient.inner_invoke("set_workout_status", { workout, status }); 
	}
	

	// From src-tauri/src/logic/app/commands.rs:82
	public static updateSettingsValue(name: string, value: string): Promise<void> {
	  return BackendClient.inner_invoke("update_settings_value", { name, value }); 
	}
	

	// From src-tauri/src/logic/workouts/commands.rs:107
	public static workoutPrintReady(workout: string): Promise<void> {
	  return BackendClient.inner_invoke("workout_print_ready", { workout }); 
	}
	

  
	private static inner_invoke<R>(method: string, payload?: InvokeArgs): Promise<R> {
		return new Promise<R>((resolve,reject)=>{
			const do_log = !BackendClient.DONT_LOG_COMMANDS.includes(method);
			if(do_log) {
				console.debug("Invoking command '"+method+"', payload: ", payload);
			}
			invoke<R>(method, payload).then((response)=>{
				if(do_log) {
					console.debug("Finished command '"+method+"', response: ", response);
				}
				resolve(response);
			}).catch((err) =>{
				if(do_log) {
					console.debug("Failed command '"+method+"', reason: ", err);
				}
				reject(err);
			});
		});
	}
}