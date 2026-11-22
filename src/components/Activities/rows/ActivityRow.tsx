import { I18nSettingsContext } from "@/context/I18nSettingsContext";
import { ActivityListItem } from "@/utils/backend/models";
import { useContext } from "react";
import EmojiEventsIcon from "@mui/icons-material/EmojiEvents";

type Props = {
    activity: ActivityListItem;
    onSelect: (timestamp: number) => void;
    onToggleCompare: (activity: ActivityListItem) => void;
    selectedForCompare: boolean;
};

export function ActivityRow({
    activity,
    selectedForCompare,
    onSelect,
    onToggleCompare,
}: Props) {
    const { translate, formatTimeDate, formatNumber } =
        useContext(I18nSettingsContext);

    return (
        <tr
            onClick={() => onSelect(activity.timestamp)}
            onContextMenu={(e) => {
                e.preventDefault();
                onToggleCompare(activity);
            }}
            className={`clickable-row ${selectedForCompare ? "row-compare-selected" : ""}`}
        >
            <td>
                {activity.has_record && (
                    <EmojiEventsIcon fontSize="small" className="trophy-icon" />
                )}
            </td>
            <td>{formatTimeDate(activity.timestamp)}</td>
            <td>
                {translate("sport_" + activity.sport) +
                    " - " +
                    translate(
                        "subsport_" + activity.sport + "_" + activity.sub_sport,
                    )}
            </td>
            <td>{activity.name}</td>
            <td>{formatNumber(activity.active_calories, 0)}</td>
            <td>{formatNumber(activity.training_load, 0)}</td>
        </tr>
    );
}
