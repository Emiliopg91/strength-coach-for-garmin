import { AppContext } from "@/context/AppContext";
import { I18nSettingsContext } from "@/context/I18nSettingsContext";
import {
    JSX,
    lazy,
    startTransition,
    Suspense,
    useContext,
    useEffect,
    useState,
} from "react";
import { NavBar, NavBarItem } from "../NavBar/NavBar";
import "@/styles/app.css";
import { Tabs } from "@/models/tabs";
import { Loading } from "../Loading/Loading";
import SettingsIcon from "@mui/icons-material/Settings";
import { Home } from "../Home/Home";
import PriorityHighIcon from "@mui/icons-material/PriorityHigh";
import { PromptDialog } from "../Prompt/PromptDialog";
import WatchIcon from "@mui/icons-material/Watch";

// Only Home is bundled eagerly; the rest is fetched in background once Home is shown
const loadActivitiesList = () => import("../Activities/ActivityList");
const loadExercisesList = () => import("../Exercises/ExercisesList");
const loadBodyMetricList = () => import("../BodyMetrics/BodyMetricList");
const loadWorkoutsList = () => import("../Workouts/WorkoutList");
const loadDevices = () => import("../Devices/Devices");
const loadSettings = () => import("../Settings/Settings");

const ActivitiesList = lazy(() =>
    loadActivitiesList().then((m) => ({ default: m.ActivitiesList })),
);
const ExercisesList = lazy(() =>
    loadExercisesList().then((m) => ({ default: m.ExercisesList })),
);
const BodyMetricList = lazy(() =>
    loadBodyMetricList().then((m) => ({ default: m.BodyMetricList })),
);
const WorkoutsList = lazy(() =>
    loadWorkoutsList().then((m) => ({ default: m.WorkoutsList })),
);
const Devices = lazy(() => loadDevices().then((m) => ({ default: m.Devices })));
const Settings = lazy(() =>
    loadSettings().then((m) => ({ default: m.Settings })),
);

export function App(): JSX.Element {
    const { appReady, progressionsAvailable } = useContext(AppContext);
    const { translate } = useContext(I18nSettingsContext);

    const [settingsOpened, setSettingsOpened] = useState(false);
    const [devicesOpened, setDevicesOpened] = useState(false);
    const [tab, setTabState] = useState(Tabs.HOME);

    useEffect(() => {
        if (!appReady) return;
        void loadActivitiesList();
        void loadExercisesList();
        void loadBodyMetricList();
        void loadWorkoutsList();
        void loadDevices();
        void loadSettings();
    }, [appReady]);

    // Transition keeps the current tab on screen while a lazy one resolves, instead of flashing the fallback
    const setTab = (newTab: Tabs) => {
        startTransition(() => setTabState(newTab));
    };

    // Even when preloaded, lazy suspends once on first render; a transition avoids hiding the tab and React's fallback throttle
    const showSettings = () => {
        startTransition(() => setSettingsOpened(true));
    };

    const closeSettings = () => {
        setSettingsOpened(false);
    };

    // Like settings: a transition avoids the fallback while the lazy dialog resolves
    const showDevices = () => {
        startTransition(() => setDevicesOpened(true));
    };

    const closeDevices = () => {
        setDevicesOpened(false);
    };

    const leftNavBarItems: NavBarItem[] = [
        {
            label: <span>{translate("home")}</span>,
            onSelected: () => {
                setTab(Tabs.HOME);
            },
            selected: tab == Tabs.HOME,
        },
        {
            label: <span>{translate("activities")}</span>,
            onSelected: () => {
                setTab(Tabs.ACTIVITIES);
            },
            selected: tab == Tabs.ACTIVITIES,
        },
        {
            label: <span>{translate("workouts")}</span>,
            onSelected: () => {
                setTab(Tabs.WORKOUTS);
            },
            selected: tab == Tabs.WORKOUTS,
        },
        {
            label: (
                <span>
                    {progressionsAvailable && (
                        <PriorityHighIcon className="warn-icon" />
                    )}
                    {translate("exercises")}
                </span>
            ),
            onSelected: () => {
                setTab(Tabs.EXERCISES);
            },
            selected: tab == Tabs.EXERCISES,
        },
        {
            label: <span>{translate("body_metrics")}</span>,
            onSelected: () => {
                setTab(Tabs.BODY_METRICS);
            },
            selected: tab == Tabs.BODY_METRICS,
        },
    ];

    const rightNavBarItems: NavBarItem[] = [
        {
            label: (
                <span>
                    <WatchIcon />
                </span>
            ),
            onSelected: () => {
                showDevices();
            },
            selected: false,
        },
        {
            label: (
                <span>
                    <SettingsIcon />
                </span>
            ),
            onSelected: () => {
                showSettings();
            },
            selected: false,
        },
    ];

    return (
        <>
            <div id="viewport">
                <Loading />
                <PromptDialog />

                {appReady && (
                    <>
                        <NavBar
                            leftItems={leftNavBarItems}
                            rightItems={rightNavBarItems}
                        />

                        <div className="list-layer">
                            <Suspense fallback={null}>
                                {tab == Tabs.HOME && <Home />}
                                {tab == Tabs.ACTIVITIES && <ActivitiesList />}
                                {tab == Tabs.EXERCISES && <ExercisesList />}
                                {tab == Tabs.WORKOUTS && <WorkoutsList />}
                                {tab == Tabs.BODY_METRICS && <BodyMetricList />}
                                {settingsOpened && (
                                    <Settings onClose={closeSettings} />
                                )}
                                {devicesOpened && (
                                    <Devices onClose={closeDevices} />
                                )}
                            </Suspense>
                        </div>
                    </>
                )}
            </div>
        </>
    );
}

export default App;
