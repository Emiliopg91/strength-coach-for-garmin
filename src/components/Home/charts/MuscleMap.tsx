import { I18nSettingsContext } from "@/context/I18nSettingsContext";
import { MuscleSets } from "@/utils/backend/models";
import { CSSProperties, MouseEvent, useContext, useRef, useState } from "react";
import { MuscleModel } from "../../Exercises/charts/MuscleModel";
import "@/styles/Home/Home.css";

// Fits both models in the 180px tall chart row
const SCALE = 0.44;
// Each set counts fully for its primary muscles and half for its secondary ones
const PRIMARY_WEIGHT = 1;
const SECONDARY_WEIGHT = 0.5;
// Gap between the pointer and the tooltip
const TOOLTIP_OFFSET = 12;

interface HoverState {
    muscles: string[];
    /// Tooltip position within the container
    style: CSSProperties;
}

interface MuscleMapProps {
    /// Sets by muscle name, only of the muscles worked
    data: Record<string, MuscleSets>;
    /// Days `data` covers, to turn its counts into sets per week
    days: number;
}

/// Front and back body maps of the muscles worked in a period: red the ones worked as primary
/// muscle, yellow the ones only worked as secondary. Hovering a muscle shows its sets per week.
export function MuscleMap({ data, days }: MuscleMapProps) {
    const { translate, formatNumber } = useContext(I18nSettingsContext);
    const containerRef = useRef<HTMLDivElement>(null);
    const [hover, setHover] = useState<HoverState | undefined>(undefined);

    const worked = Object.entries(data);
    const primary = worked
        .filter(([, sets]) => sets.primary > 0)
        .map(([muscle]) => muscle);
    const secondary = worked
        .filter(([, sets]) => sets.primary == 0 && sets.secondary > 0)
        .map(([muscle]) => muscle);

    const onHover = (
        regionMuscles: string[] | undefined,
        event: MouseEvent,
    ) => {
        const rect = containerRef.current?.getBoundingClientRect();
        // Only the muscles worked: a region with none of them shows no tooltip
        const muscles = regionMuscles?.filter((m) => Object.hasOwn(data, m));
        if (!muscles?.length || !rect) {
            setHover(undefined);
            return;
        }
        const x = event.clientX - rect.left;
        // Opens towards the center of the window, so it never overflows its right edge
        const horizontal =
            event.clientX > window.innerWidth / 2
                ? { right: rect.width - x + TOOLTIP_OFFSET }
                : { left: x + TOOLTIP_OFFSET };
        setHover({
            muscles,
            style: {
                ...horizontal,
                top: event.clientY - rect.top + TOOLTIP_OFFSET,
            },
        });
    };

    const perWeek = (sets: number) => formatNumber((sets * 7) / days, 1);

    // Weighted sets per week: worked muscles always have some
    const describe = (muscle: string) => {
        const sets = data[muscle];
        const weighted =
            sets.primary * PRIMARY_WEIGHT + sets.secondary * SECONDARY_WEIGHT;
        return `${translate("muscle_" + muscle)}: ${translate("sets_per_week", [
            perWeek(weighted),
        ])}`;
    };

    return (
        <div className="chart-container muscle-map" ref={containerRef}>
            <MuscleModel
                primary={primary}
                secondary={secondary}
                scale={SCALE}
                onHover={onHover}
            />
            {hover && (
                <div
                    className="heatmap-tooltip muscle-map-tooltip"
                    style={hover.style}
                >
                    {hover.muscles.map((muscle) => (
                        <div key={muscle}>{describe(muscle)}</div>
                    ))}
                </div>
            )}
        </div>
    );
}
