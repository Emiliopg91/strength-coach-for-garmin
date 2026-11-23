import { AppContext } from "@/context/AppContext";
import { I18nSettingsContext } from "@/context/I18nSettingsContext";
import { useBackendEvent } from "@/hooks/useBackendEvent";
import { useCurrentDay } from "@/hooks/useCurrentDay";
import { useLoadingTask } from "@/hooks/useLoadingTask";
import { BackendClient } from "@/utils/backend/client";
import {
    ActivityFrontDetails,
    ActivityUtils,
    WorkoutLoad,
} from "@/utils/ActivityUtils";
import { usePickerAdapter } from "@mui/x-date-pickers/hooks";
import { useContext, useEffect, useEffectEvent, useState } from "react";
import "@/styles/Home/Home.css";
import "@/styles/Activities/ActivityModal.css";
import {
    ActivityListItem,
    MuscleSets,
    WorkoutDetails,
    WorkoutListItem,
} from "@/utils/backend/models";
import { BackendListener } from "@/utils/backend/listener";
import { ActivityModal } from "../Activities/modals/ActivityModal";
import { WorkoutModal } from "../Workouts/modals/WorkoutModal";
import { Heatmap } from "./charts/Heatmap";
import { MuscleMap } from "./charts/MuscleMap";
import { WorkloadChart } from "./charts/WorkloadChart";
import { WorkoutStepsModal } from "../Workouts/modals/WorkoutStepsModal";
import { WorkoutActionsMenu } from "../Workouts/menus/WorkoutActionsMenu";
import { Button } from "@mui/material";
import { ReportModal } from "../Report/modals/ReportModal";
import { ImportActivitiesMenu } from "../Activities/menus/ImportActivitiesMenu";
import { differenceInCalendarDays } from "date-fns";

interface PeriodStats {
    activities: number;
    time: number;
    kcal: number;
    load: number;
}

interface NextWorkout {
    /// Planned date (local midnight, seconds); undefined when it is just a suggestion
    date?: number;
    details: WorkoutListItem;
}

const PERIODS = ["today", "thisWeek", "previousWeek"] as const;
type Period = (typeof PERIODS)[number];
type HomeStats = Record<Period, PeriodStats>;

// Days the muscle map covers
const MONTH_DAYS = 30;

const emptyPeriod = (): PeriodStats => ({
    activities: 0,
    time: 0,
    kcal: 0,
    load: 0,
});

const emptyStats = (): HomeStats => ({
    today: emptyPeriod(),
    thisWeek: emptyPeriod(),
    previousWeek: emptyPeriod(),
});

const addActivity = (stats: PeriodStats, activity: ActivityListItem) => {
    stats.activities += 1;
    stats.time += activity.total_elapsed_time;
    stats.kcal += activity.active_calories;
    stats.load += activity.training_load;
};

// Activities must be sorted by timestamp, most recent first
const calculateStats = (
    activities: ActivityListItem[],
    today: number,
    adapter: ReturnType<typeof usePickerAdapter>,
): HomeStats => {
    const stats = emptyStats();

    const startOfWeek = adapter.startOfWeek(adapter.startOfDay(new Date()));
    const thisWeekLimit = startOfWeek.getTime() / 1000;
    const prevWeekLimit = adapter.addWeeks(startOfWeek, -1).getTime() / 1000;

    for (const activity of activities) {
        if (activity.timestamp < prevWeekLimit) {
            break;
        }
        if (activity.timestamp >= thisWeekLimit) {
            addActivity(stats.thisWeek, activity);
            if (activity.timestamp >= today) {
                addActivity(stats.today, activity);
            }
        } else {
            addActivity(stats.previousWeek, activity);
        }
    }

    return stats;
};

export function Home() {
    const { activitiesVersion } = useContext(AppContext);
    const withLoading = useLoadingTask();
    const {
        translate,
        fromKg,
        formatDuration,
        formatTimeDate,
        formatWeekday,
        formatNumber,
    } = useContext(I18nSettingsContext);
    const adapter = usePickerAdapter();

    const day = useCurrentDay();
    const [availableData, setAvailableData] = useState(false);
    const [workload, setWorkload] = useState<WorkoutLoad[]>([]);
    const [stats, setStats] = useState<HomeStats>(emptyStats);
    const [heatMapData, setHeatMapData] = useState<
        [number, number][][] | undefined
    >(undefined);
    const [muscleSets, setMuscleSets] = useState<Record<string, MuscleSets>>(
        {},
    );

    const [workout, setWorkout] = useState<NextWorkout | undefined>(undefined);
    const [rest, setRest] = useState(false);
    const [workoutDetails, setWorkoutDetails] = useState<
        WorkoutDetails | undefined
    >(undefined);
    const [activity, setLastActivity] = useState<ActivityListItem | undefined>(
        undefined,
    );
    const [activityDetails, setActivityDetails] = useState<
        ActivityFrontDetails | undefined
    >(undefined);
    const [workoutEdit, setWorkoutEdit] = useState<WorkoutDetails | undefined>(
        undefined,
    );
    const [reportModal, setReportModal] = useState(false);

    const refresh = () => {
        const today =
            new Date(new Date().setHours(0, 0, 0, 0)).getTime() / 1000;
        withLoading(
            BackendClient.getActivities(
                today - ActivityUtils.CHRONIC_DAYS * 2 * 24 * 60 * 60,
            ),
        ).then((activities) => {
            setAvailableData(activities.length > 0);

            if (activities.length > 0) {
                const workout_data =
                    ActivityUtils.calculateWorkoutLoad(activities);
                const overreaching = ActivityUtils.isOverreaching(workout_data);
                const newStats = calculateStats(activities, today, adapter);

                withLoading(BackendClient.getHeatmapData()).then(
                    setHeatMapData,
                );
                withLoading(
                    BackendClient.getMuscleSets(
                        today - (MONTH_DAYS - 1) * 24 * 60 * 60,
                    ),
                ).then(setMuscleSets);

                Promise.all([
                    withLoading(BackendClient.getNextPlannedWorkout()),
                    withLoading(BackendClient.getWorkoutList()),
                ]).then(([next, workouts]) => {
                    const planned =
                        next !== null
                            ? workouts.find((w) => w.id == next.workout)
                            : undefined;

                    if (next !== null && planned) {
                        setRest(false);
                        setWorkout({ date: next.date, details: planned });
                        return;
                    }

                    // No planned workout: fall back to the activity suggestion
                    setRest(newStats.today.load > 50 || overreaching);

                    const filtered = workouts.filter((w) => w.enabled);
                    if (!overreaching && filtered.length > 0) {
                        // Never-done workouts take priority, otherwise the least recent one
                        let oldest = filtered.find(
                            (w) => w.latest_activity == null,
                        );
                        if (oldest === undefined) {
                            oldest = filtered[0];
                            for (const w of filtered) {
                                if (
                                    oldest.latest_activity! > w.latest_activity!
                                ) {
                                    oldest = w;
                                }
                            }
                        }
                        setWorkout({ details: oldest });
                    } else {
                        setWorkout(undefined);
                    }
                });

                setStats(newStats);

                setWorkload(workout_data);

                setLastActivity(activities[0]);
            }
        });
    };

    const isPlanned = workout?.date !== undefined;

    // "Today", the day of the week within a week, or how many days away the planned workout is
    const formatPlannedDay = (date: number) => {
        const diff = differenceInCalendarDays(date * 1000, new Date());
        if (diff <= 0) return translate("today");
        if (diff <= 7) return formatWeekday(date);
        return translate("in_n_days", [String(diff)]);
    };

    const getActivityDetails = (timestamp: number) => {
        withLoading(BackendClient.getActivityDetails(timestamp)).then(
            (details) => {
                setActivityDetails(
                    ActivityUtils.detailsFromBackend(details, fromKg),
                );
            },
        );
    };

    const getWorkoutDetails = (id: number) => {
        withLoading(BackendClient.getWorkoutDetails(id)).then(
            setWorkoutDetails,
        );
    };

    useBackendEvent(BackendListener.onActivityLocationUpdate, (data) => {
        setLastActivity((prev) =>
            prev && prev.timestamp == data.activity
                ? { ...prev, name: data.location }
                : prev,
        );
    });

    useBackendEvent(BackendListener.onAddedWorkoutSteps, (ids) => {
        setWorkout((prev) =>
            prev && ids.includes(prev.details.id)
                ? { ...prev, details: { ...prev.details, has_steps: true } }
                : prev,
        );
    });

    const onRefreshTrigger = useEffectEvent(refresh);

    useEffect(() => {
        onRefreshTrigger();
    }, [activitiesVersion, adapter, day]);

    const openEdit = (workout: number) => {
        withLoading(BackendClient.getWorkoutDetails(workout)).then(
            (details) => {
                setWorkoutDetails(undefined);
                setWorkoutEdit(details);
            },
        );
    };

    return (
        <>
            {!availableData && (
                <>
                    <h1 id="no-available-data">
                        {translate("no_available_data")}
                    </h1>
                    <h3 id="import-activities">
                        {translate("import_activity_to_begin")}
                    </h3>
                </>
            )}
            {availableData && (
                <>
                    <div className="dashboard-box">
                        <fieldset>
                            <legend>{translate("training_status")}</legend>
                            <div className="training-status-charts">
                                <div>
                                    {workload.length > 0 && (
                                        <WorkloadChart
                                            workload={workload}
                                            minDate={workload[0].date}
                                        />
                                    )}
                                </div>
                                <Heatmap data={heatMapData} />
                            </div>
                            <div className="training-status-summary">
                                <table id="last-days">
                                    <colgroup>
                                        <col />
                                        <col />
                                        <col />
                                        <col />
                                    </colgroup>
                                    <thead>
                                        <tr>
                                            <th></th>
                                            <th>{translate("today")}</th>
                                            <th>{translate("this_week")}</th>
                                            <th>
                                                {translate("previous_week")}
                                            </th>
                                        </tr>
                                    </thead>
                                    <tbody>
                                        <tr>
                                            <td>{translate("activities")}</td>
                                            {PERIODS.map((p) => (
                                                <td key={p}>
                                                    {formatNumber(
                                                        stats[p].activities,
                                                        0,
                                                    )}
                                                </td>
                                            ))}
                                        </tr>
                                    </tbody>
                                    <tbody>
                                        <tr>
                                            <td>{translate("workout_load")}</td>
                                            {PERIODS.map((p) => (
                                                <td key={p}>
                                                    {formatNumber(
                                                        stats[p].activities > 0
                                                            ? stats[p].load /
                                                                  stats[p]
                                                                      .activities
                                                            : 0,
                                                        0,
                                                    ) +
                                                        " / " +
                                                        formatNumber(
                                                            stats[p].load,
                                                            0,
                                                        )}
                                                </td>
                                            ))}
                                        </tr>
                                    </tbody>
                                    <tbody>
                                        <tr>
                                            <td>{translate("active_time")}</td>
                                            {PERIODS.map((p) => (
                                                <td key={p}>
                                                    {formatDuration(
                                                        stats[p].time,
                                                    )}
                                                </td>
                                            ))}
                                        </tr>
                                    </tbody>
                                    <tbody>
                                        <tr>
                                            <td>
                                                {translate("active_calories")}
                                            </td>
                                            {PERIODS.map((p) => (
                                                <td key={p}>
                                                    {formatNumber(
                                                        stats[p].kcal,
                                                        0,
                                                    )}{" "}
                                                    Kcal
                                                </td>
                                            ))}
                                        </tr>
                                    </tbody>
                                </table>
                                <MuscleMap
                                    data={muscleSets}
                                    days={MONTH_DAYS}
                                />
                            </div>
                        </fieldset>
                    </div>
                    <div className="dashboard-box">
                        <fieldset>
                            <legend>{translate("last_activity")}</legend>
                            <div className="list-layer">
                                <table>
                                    <thead>
                                        <tr>
                                            <th className="text-center">
                                                {translate("date")}
                                            </th>
                                            <th className="text-center">
                                                {translate("sport")}
                                            </th>
                                            <th className="text-center">
                                                {translate("name")}
                                            </th>
                                            <th className="text-center">
                                                {translate("active_calories")}
                                            </th>
                                            <th className="text-center">
                                                {translate("workout_load")}
                                            </th>
                                        </tr>
                                    </thead>

                                    <tbody>
                                        <tr
                                            className="clickable-row"
                                            onClick={() =>
                                                getActivityDetails(
                                                    activity!.timestamp,
                                                )
                                            }
                                        >
                                            <td>
                                                {formatTimeDate(
                                                    activity!.timestamp,
                                                )}
                                            </td>
                                            <td>
                                                {translate(
                                                    "sport_" + activity!.sport,
                                                ) +
                                                    " - " +
                                                    translate(
                                                        "subsport_" +
                                                            activity!.sport +
                                                            "_" +
                                                            activity!.sub_sport,
                                                    )}
                                            </td>
                                            <td>{activity!.name}</td>
                                            <td>
                                                {formatNumber(
                                                    activity!.active_calories,
                                                    0,
                                                )}
                                            </td>
                                            <td>
                                                {formatNumber(
                                                    activity!.training_load,
                                                    0,
                                                )}
                                            </td>
                                        </tr>
                                    </tbody>
                                </table>
                            </div>
                        </fieldset>
                    </div>
                    <div className="dashboard-box">
                        <fieldset>
                            <legend>
                                {translate(
                                    isPlanned
                                        ? "next_planned_workout"
                                        : "activity_suggestion",
                                )}
                            </legend>
                            {!isPlanned && rest && (
                                <div style={{ textAlign: "center" }}>
                                    <p>{translate("rest_recommended")}</p>
                                </div>
                            )}
                            {workout && (isPlanned || !rest) && (
                                <div className="list-layer">
                                    <table>
                                        <thead>
                                            <tr>
                                                {isPlanned && (
                                                    <th className="text-center">
                                                        {translate("date")}
                                                    </th>
                                                )}
                                                <th className="text-center">
                                                    {translate("workout")}
                                                </th>
                                                <th className="text-center">
                                                    {translate(
                                                        "latest_activity",
                                                    )}
                                                </th>
                                                <th className="text-center">
                                                    {translate(
                                                        "average_duration",
                                                    )}
                                                </th>
                                                <th className="text-center">
                                                    {translate("avg_workload")}
                                                </th>
                                                <th></th>
                                            </tr>
                                        </thead>
                                        <tbody>
                                            <tr
                                                className="clickable-row"
                                                onClick={() =>
                                                    getWorkoutDetails(
                                                        workout.details.id,
                                                    )
                                                }
                                            >
                                                {workout.date !== undefined && (
                                                    <td className="text-center">
                                                        {formatPlannedDay(
                                                            workout.date,
                                                        )}
                                                    </td>
                                                )}
                                                <td className="text-center">
                                                    {workout.details.name
                                                        .length > 0 && (
                                                        <span>
                                                            {
                                                                workout.details
                                                                    .name
                                                            }
                                                        </span>
                                                    )}
                                                    {workout.details.name
                                                        .length == 0 && (
                                                        <span>
                                                            {translate("other")}
                                                        </span>
                                                    )}
                                                </td>
                                                <td>
                                                    {workout.details
                                                        .latest_activity &&
                                                        formatTimeDate(
                                                            workout.details
                                                                .latest_activity,
                                                        )}
                                                </td>
                                                <td>
                                                    {formatDuration(
                                                        workout.details
                                                            .avg_time,
                                                    )}
                                                </td>
                                                <td>
                                                    {formatNumber(
                                                        workout.details
                                                            .avg_load,
                                                        0,
                                                    )}
                                                </td>
                                                <td
                                                    onClick={(e) =>
                                                        e.stopPropagation()
                                                    }
                                                >
                                                    {workout.details
                                                        .has_steps && (
                                                        <WorkoutActionsMenu
                                                            workoutId={
                                                                workout.details
                                                                    .id
                                                            }
                                                            onOpenEdit={() =>
                                                                openEdit(
                                                                    workout
                                                                        .details
                                                                        .id,
                                                                )
                                                            }
                                                            send={true}
                                                            dense
                                                        />
                                                    )}
                                                </td>
                                            </tr>
                                        </tbody>
                                    </table>
                                </div>
                            )}
                        </fieldset>
                    </div>
                </>
            )}

            <div>
                {activityDetails && (
                    <ActivityModal
                        activity={activityDetails}
                        onClose={() => setActivityDetails(undefined)}
                        onUpdate={() => {
                            refresh();
                        }}
                    />
                )}
            </div>

            <div>
                {workoutDetails && (
                    <WorkoutModal
                        workout={workoutDetails}
                        showEnable={false}
                        onClose={() => setWorkoutDetails(undefined)}
                        onOpenEdit={() => {
                            openEdit(workoutDetails.id);
                        }}
                    />
                )}
            </div>

            <div>
                {workoutEdit && (
                    <WorkoutStepsModal
                        workout={workoutEdit}
                        isEdit={true}
                        onClose={() => setWorkoutEdit(undefined)}
                    />
                )}
            </div>

            <div>
                {reportModal && (
                    <ReportModal
                        onClose={() => {
                            setReportModal(false);
                        }}
                    />
                )}
            </div>

            <div
                style={{ display: "flex", gap: 5 }}
                className="list-action-bar equal-width-children"
            >
                <ImportActivitiesMenu onImported={refresh} />
                <Button
                    variant="contained"
                    onClick={() => {
                        setReportModal(true);
                    }}
                >
                    {translate("generate_report")}
                </Button>
            </div>
        </>
    );
}
