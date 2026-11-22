import { AppContext } from "@/context/AppContext";
import { useBackendEvent } from "@/hooks/useBackendEvent";
import { useBackendList } from "@/hooks/useBackendList";
import { useLoadingTask } from "@/hooks/useLoadingTask";
import { I18nSettingsContext } from "@/context/I18nSettingsContext";
import { BackendClient } from "@/utils/backend/client";
import { ActivityListItem } from "@/utils/backend/models";
import { useContext, useState } from "react";
import { ActivityModal } from "./modals/ActivityModal";
import { ActivityRow } from "./rows/ActivityRow";
import { BackendListener } from "@/utils/backend/listener";
import { ActivityFrontDetails, ActivityUtils } from "@/utils/ActivityUtils";
import "@/styles/Activities/ActivityList.css";
import { Button } from "@mui/material";
import { ActivitiesCompareModal } from "./modals/ActivitiesCompareModal";
import { useFilterList } from "@/hooks/useFilterList";
import { useBatchedRows } from "@/hooks/useBatchedRows";
import { ImportActivitiesMenu } from "./menus/ImportActivitiesMenu";

export function ActivitiesList() {
    const { activitiesVersion } = useContext(AppContext);
    const withLoading = useLoadingTask();
    const { translate, fromKg, formatTimeDate } =
        useContext(I18nSettingsContext);
    const [activities, setActivities, refreshList] =
        useBackendList<ActivityListItem>(
            () => BackendClient.getActivities(null),
            [activitiesVersion],
        );
    const [activityDetails, setActivityDetails] = useState<
        ActivityFrontDetails | undefined
    >(undefined);
    const [toCompare, setToCompare] = useState<number[]>([]);
    const [toCompareName, setToCompareName] = useState<string | undefined>(
        undefined,
    );
    const [comparingActivities, setComparingActivities] = useState<
        [ActivityFrontDetails, ActivityFrontDetails] | undefined
    >(undefined);

    useBackendEvent(BackendListener.onActivityLocationUpdate, (data) => {
        setActivities((prev) => {
            return prev.map((item) => {
                if (item.timestamp !== data.activity) return item;
                return { ...item, name: data.location };
            });
        });
    });

    const getActivityDetails = (timestamp: number) => {
        withLoading(BackendClient.getActivityDetails(timestamp)).then(
            (details) => {
                setActivityDetails(
                    ActivityUtils.detailsFromBackend(details, fromKg),
                );
            },
        );
    };

    const toggleSelect = (activity: ActivityListItem) => {
        if (
            (activity.has_sets && toCompareName === undefined) ||
            toCompareName == activity.name
        ) {
            const was0 = toCompare.length == 0;
            if (toCompare.includes(activity.timestamp)) {
                const toComp = [...toCompare].filter(
                    (date) => date != activity.timestamp,
                );
                if (toComp.length == 0) {
                    setToCompareName(undefined);
                }
                setToCompare(toComp);
            } else {
                if (toCompare.length < 2) {
                    setToCompare([...toCompare, activity.timestamp]);
                    if (was0) {
                        setToCompareName(activity.name);
                    }
                }
            }
        }
    };

    const goToCompare = () => {
        withLoading(
            Promise.all(
                toCompare
                    .toSorted((a, b) => a - b)
                    .map((d) => BackendClient.getActivityDetails(d)),
            ),
        ).then(([first, second]) => {
            setComparingActivities([
                ActivityUtils.detailsFromBackend(first, fromKg),
                ActivityUtils.detailsFromBackend(second, fromKg),
            ]);
        });
    };

    const [filter, filteredActivities] = useFilterList(
        activities,
        (activity) => {
            return [
                formatTimeDate(activity.timestamp),
                activity.name,
                translate("sport_" + activity.sport) +
                    " - " +
                    translate(
                        "subsport_" + activity.sport + "_" + activity.sub_sport,
                    ),
            ];
        },
    );
    const [visibleActivities, sentinel] = useBatchedRows(filteredActivities);

    return (
        <>
            {filter}
            <div className="list-layer">
                <table>
                    <thead>
                        <tr>
                            <th></th>
                            <th className="text-center">{translate("date")}</th>
                            <th className="text-center">
                                {translate("sport")}
                            </th>
                            <th className="text-center">{translate("name")}</th>
                            <th className="text-center">
                                {translate("active_calories")}
                            </th>
                            <th className="text-center">
                                {translate("workout_load")}
                            </th>
                        </tr>
                    </thead>

                    <tbody>
                        {visibleActivities.map((activity) => (
                            <ActivityRow
                                key={activity.timestamp}
                                activity={activity}
                                onSelect={getActivityDetails}
                                selectedForCompare={toCompare.includes(
                                    activity.timestamp,
                                )}
                                onToggleCompare={toggleSelect}
                            />
                        ))}
                        {sentinel}
                    </tbody>
                </table>
            </div>

            <div>
                {activityDetails && (
                    <ActivityModal
                        activity={activityDetails}
                        onClose={() => setActivityDetails(undefined)}
                        onUpdate={() => refreshList()}
                    />
                )}
            </div>

            {toCompare.length == 2 && (
                <div className="list-action-bar list-action-bar-row">
                    <Button
                        id="add-measure-button"
                        variant="contained"
                        className="full-width-button"
                        onClick={goToCompare}
                    >
                        {translate("compare")}
                    </Button>
                </div>
            )}
            {comparingActivities && (
                <ActivitiesCompareModal
                    activities={comparingActivities}
                    onClose={() => setComparingActivities(undefined)}
                />
            )}

            <div
                style={{ display: "flex", gap: 5 }}
                className="list-action-bar equal-width-children"
            >
                <ImportActivitiesMenu onImported={refreshList} />
            </div>
        </>
    );
}
