//Auto generated file, do not edit manually

import { invoke, InvokeArgs } from "@tauri-apps/api/core";

import { ActivityDetails, ActivityListItem, ActivityStepsUpdate, AppEnvironment, BodyMetricListItem, CloudProvider, DeviceListItem, ExerciseDetails, ExerciseListItem, MuscleSets, PercentLoadBase, ProgressionSuggestion, ReportData, Settings, Workout, WorkoutDetails, WorkoutListItem, WorkoutPlanListItem } from "./models";

export class BackendClient {

    private static DONT_LOG_COMMANDS: string[] = ["answer_prompt"];

	// From src-tauri/src/logic/body_metrics/commands.rs:21
	public static addBodyMeasures(measures: BodyMetricListItem): Promise<void> {
	  return BackendClient.inner_invoke("add_body_measures", { measures }); 
	}
	

	// From src-tauri/src/logic/workouts/commands.rs:121
	public static addPlannedWorkout(date: number, workout: number): Promise<void> {
	  return BackendClient.inner_invoke("add_planned_workout", { date, workout }); 
	}
	

	// From src-tauri/src/logic/prompts/commands.rs:13
	public static answerPrompt(id: number, values: string[] | null): Promise<void> {
	  return BackendClient.inner_invoke("answer_prompt", { id, values }); 
	}
	

	// From src-tauri/src/logic/exercises/commands.rs:127
	public static applyProgressionSuggestion(category: number, id: number, suggestion: ProgressionSuggestion): Promise<void> {
	  return BackendClient.inner_invoke("apply_progression_suggestion", { category, id, suggestion }); 
	}
	

	// From src-tauri/src/logic/exercises/commands.rs:117
	public static availableAnyProgressionSuggestions(): Promise<boolean> {
	  return BackendClient.inner_invoke("available_any_progression_suggestions"); 
	}
	

	// From src-tauri/src/logic/connect/commands.rs:36
	public static connectLoggedUser(): Promise<string | null> {
	  return BackendClient.inner_invoke("connect_logged_user"); 
	}
	

	// From src-tauri/src/logic/connect/commands.rs:47
	public static connectLogin(): Promise<string | null> {
	  return BackendClient.inner_invoke("connect_login"); 
	}
	

	// From src-tauri/src/logic/connect/commands.rs:57
	public static connectLogout(): Promise<void> {
	  return BackendClient.inner_invoke("connect_logout"); 
	}
	

	// From src-tauri/src/logic/body_metrics/commands.rs:33
	public static deleteBodyMetric(date: number): Promise<void> {
	  return BackendClient.inner_invoke("delete_body_metric", { date }); 
	}
	

	// From src-tauri/src/logic/workouts/commands.rs:114
	public static deletePlannedWorkout(date: number, workout: number): Promise<void> {
	  return BackendClient.inner_invoke("delete_planned_workout", { date, workout }); 
	}
	

	// From src-tauri/src/logic/activities/commands.rs:43
	public static downloadActivity(timestamp: number): Promise<void> {
	  return BackendClient.inner_invoke("download_activity", { timestamp }); 
	}
	

	// From src-tauri/src/logic/export/commands.rs:30
	public static exportDatabase(): Promise<void> {
	  return BackendClient.inner_invoke("export_database"); 
	}
	

	// From src-tauri/src/logic/activities/commands.rs:16
	public static getActivities(limit: number | null): Promise<ActivityListItem[]> {
	  return BackendClient.inner_invoke("get_activities", { limit }); 
	}
	

	// From src-tauri/src/logic/activities/commands.rs:29
	public static getActivityDetails(timestamp: number): Promise<ActivityDetails> {
	  return BackendClient.inner_invoke("get_activity_details", { timestamp }); 
	}
	

	// From src-tauri/src/logic/body_metrics/commands.rs:10
	public static getBodyMeasures(): Promise<BodyMetricListItem[]> {
	  return BackendClient.inner_invoke("get_body_measures"); 
	}
	

	// From src-tauri/src/logic/app/commands.rs:31
	public static getEnvironment(): Promise<AppEnvironment> {
	  return BackendClient.inner_invoke("get_environment"); 
	}
	

	// From src-tauri/src/logic/exercises/commands.rs:28
	public static getExerciseDetails(category: number, id: number): Promise<ExerciseDetails> {
	  return BackendClient.inner_invoke("get_exercise_details", { category, id }); 
	}
	

	// From src-tauri/src/logic/exercises/commands.rs:17
	public static getExercises(): Promise<ExerciseListItem[]> {
	  return BackendClient.inner_invoke("get_exercises"); 
	}
	

	// From src-tauri/src/logic/exercises/commands.rs:41
	public static getExercisesCatalog(): Promise<Record<number, number[]>> {
	  return BackendClient.inner_invoke("get_exercises_catalog"); 
	}
	

	// From src-tauri/src/logic/activities/commands.rs:83
	public static getHeatmapData(): Promise<[number, number][][]> {
	  return BackendClient.inner_invoke("get_heatmap_data"); 
	}
	

	// From src-tauri/src/logic/activities/commands.rs:92
	public static getMuscleSets(since: number): Promise<MuscleSets[]> {
	  return BackendClient.inner_invoke("get_muscle_sets", { since }); 
	}
	

	// From src-tauri/src/logic/workouts/commands.rs:128
	public static getNextPlannedWorkout(): Promise<WorkoutPlanListItem | null> {
	  return BackendClient.inner_invoke("get_next_planned_workout"); 
	}
	

	// From src-tauri/src/logic/workouts/commands.rs:97
	public static getPercentLoadBases(): Promise<PercentLoadBase[]> {
	  return BackendClient.inner_invoke("get_percent_load_bases"); 
	}
	

	// From src-tauri/src/logic/workouts/commands.rs:107
	public static getPlannedWorkouts(): Promise<WorkoutPlanListItem[]> {
	  return BackendClient.inner_invoke("get_planned_workouts"); 
	}
	

	// From src-tauri/src/logic/exercises/commands.rs:95
	public static getProgressionStatus(category: number, id: number): Promise<ProgressionSuggestion | null> {
	  return BackendClient.inner_invoke("get_progression_status", { category, id }); 
	}
	

	// From src-tauri/src/logic/exercises/commands.rs:106
	public static getProgressionSuggestion(category: number, id: number): Promise<ProgressionSuggestion | null> {
	  return BackendClient.inner_invoke("get_progression_suggestion", { category, id }); 
	}
	

	// From src-tauri/src/logic/devices/commands.rs:34
	public static getRegisteredDevices(): Promise<DeviceListItem[]> {
	  return BackendClient.inner_invoke("get_registered_devices"); 
	}
	

	// From src-tauri/src/logic/report/commands.rs:28
	public static getReportData(from: number, to: number): Promise<ReportData> {
	  return BackendClient.inner_invoke("get_report_data", { from, to }); 
	}
	

	// From src-tauri/src/logic/app/commands.rs:15
	public static getSettings(): Promise<Settings> {
	  return BackendClient.inner_invoke("get_settings"); 
	}
	

	// From src-tauri/src/logic/app/commands.rs:49
	public static getTranslations(): Promise<Record<string, string>> {
	  return BackendClient.inner_invoke("get_translations"); 
	}
	

	// From src-tauri/src/logic/workouts/commands.rs:26
	public static getWorkoutDetails(id: number): Promise<WorkoutDetails> {
	  return BackendClient.inner_invoke("get_workout_details", { id }); 
	}
	

	// From src-tauri/src/logic/workouts/commands.rs:15
	public static getWorkoutList(): Promise<WorkoutListItem[]> {
	  return BackendClient.inner_invoke("get_workout_list"); 
	}
	

	// From src-tauri/src/logic/connect/commands.rs:12
	public static importFromConnect(): Promise<number> {
	  return BackendClient.inner_invoke("import_from_connect"); 
	}
	

	// From src-tauri/src/logic/devices/commands.rs:27
	public static importFromDevice(serial: string): Promise<number> {
	  return BackendClient.inner_invoke("import_from_device", { serial }); 
	}
	

	// From src-tauri/src/logic/activities/commands.rs:72
	public static importFromFiles(): Promise<number> {
	  return BackendClient.inner_invoke("import_from_files"); 
	}
	

	// From src-tauri/src/logic/connect/commands.rs:25
	public static importWeightFromConnect(): Promise<number> {
	  return BackendClient.inner_invoke("import_weight_from_connect"); 
	}
	

	// From src-tauri/src/logic/export/commands.rs:9
	public static isCloudUploadAvailable(): Promise<boolean> {
	  return BackendClient.inner_invoke("is_cloud_upload_available"); 
	}
	

	// From src-tauri/src/logic/report/commands.rs:10
	public static loadReport(from: number, to: number): Promise<void> {
	  return BackendClient.inner_invoke("load_report", { from, to }); 
	}
	

	// From src-tauri/src/logic/workouts/commands.rs:79
	public static loadWorkoutPage(workout: number): Promise<void> {
	  return BackendClient.inner_invoke("load_workout_page", { workout }); 
	}
	

	// From src-tauri/src/logic/app/commands.rs:24
	public static notifyFrontendReady(): Promise<void> {
	  return BackendClient.inner_invoke("notify_frontend_ready"); 
	}
	

	// From src-tauri/src/logic/report/commands.rs:19
	public static reportReady(): Promise<void> {
	  return BackendClient.inner_invoke("report_ready"); 
	}
	

	// From src-tauri/src/logic/activities/commands.rs:57
	public static saveActivityChanges(details: ActivityStepsUpdate): Promise<void> {
	  return BackendClient.inner_invoke("save_activity_changes", { details }); 
	}
	

	// From src-tauri/src/logic/exercises/commands.rs:143
	public static saveExerciseNotes(category: number, id: number, notes: string): Promise<void> {
	  return BackendClient.inner_invoke("save_exercise_notes", { category, id, notes }); 
	}
	

	// From src-tauri/src/logic/workouts/commands.rs:65
	public static saveWorkout(edit: boolean, workout: Workout): Promise<void> {
	  return BackendClient.inner_invoke("save_workout", { edit, workout }); 
	}
	

	// From src-tauri/src/logic/connect/commands.rs:66
	public static sendToConnect(workout: number, serial: string): Promise<void> {
	  return BackendClient.inner_invoke("send_to_connect", { workout, serial }); 
	}
	

	// From src-tauri/src/logic/devices/commands.rs:12
	public static sendToDevice(workout: number, serial: string): Promise<void> {
	  return BackendClient.inner_invoke("send_to_device", { workout, serial }); 
	}
	

	// From src-tauri/src/logic/exercises/commands.rs:64
	public static setExerciseProgression(category: number, id: number, value: boolean): Promise<void> {
	  return BackendClient.inner_invoke("set_exercise_progression", { category, id, value }); 
	}
	

	// From src-tauri/src/logic/exercises/commands.rs:80
	public static setExerciseProgressionParameters(category: number, id: number, reps: string, weight: number): Promise<void> {
	  return BackendClient.inner_invoke("set_exercise_progression_parameters", { category, id, reps, weight }); 
	}
	

	// From src-tauri/src/logic/exercises/commands.rs:50
	public static setExerciseUnilateral(category: number, id: number, value: boolean): Promise<void> {
	  return BackendClient.inner_invoke("set_exercise_unilateral", { category, id, value }); 
	}
	

	// From src-tauri/src/logic/workouts/commands.rs:48
	public static setWorkoutProgression(workout: number, value: boolean): Promise<void> {
	  return BackendClient.inner_invoke("set_workout_progression", { workout, value }); 
	}
	

	// From src-tauri/src/logic/workouts/commands.rs:38
	public static setWorkoutStatus(workout: number, status: boolean): Promise<void> {
	  return BackendClient.inner_invoke("set_workout_status", { workout, status }); 
	}
	

	// From src-tauri/src/logic/app/commands.rs:42
	public static updateSettingsValue(name: string, value: string): Promise<void> {
	  return BackendClient.inner_invoke("update_settings_value", { name, value }); 
	}
	

	// From src-tauri/src/logic/export/commands.rs:18
	public static uploadToCloud(provider: CloudProvider): Promise<void> {
	  return BackendClient.inner_invoke("upload_to_cloud", { provider }); 
	}
	

	// From src-tauri/src/logic/workouts/commands.rs:88
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