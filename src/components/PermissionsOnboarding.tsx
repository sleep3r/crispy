import React, { useState } from "react";
import { usePermissions } from "../hooks/usePermissions";
import { Mic, Monitor, CheckCircle2, Loader2, Shield } from "lucide-react";

interface PermissionCardProps {
    icon: React.ElementType;
    title: string;
    description: string;
    granted: boolean;
    onGrant: () => Promise<void>;
}

const PermissionCard: React.FC<PermissionCardProps> = ({
    icon: Icon,
    title,
    description,
    granted,
    onGrant,
}) => {
    const [requesting, setRequesting] = useState(false);

    const handleGrant = async () => {
        setRequesting(true);
        try {
            await onGrant();
        } finally {
            setRequesting(false);
        }
    };

    return (
        <div
            className={`
        relative w-full rounded-2xl border p-6 transition-all duration-300
        ${granted
                    ? "border-green-500/30 bg-green-500/5"
                    : "border-mid-gray/20 bg-mid-gray/5"
                }
      `}
        >
            <div className="flex items-start gap-4">
                <div
                    className={`
            flex items-center justify-center w-12 h-12 rounded-xl shrink-0 transition-colors duration-300
            ${granted
                            ? "bg-green-500/15 text-green-500"
                            : "bg-slider-fill/15 text-slider-fill"
                        }
          `}
                >
                    <Icon size={24} />
                </div>

                <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2 mb-1">
                        <h3 className="text-base font-semibold text-text">{title}</h3>
                        {granted && (
                            <CheckCircle2 size={18} className="text-green-500 shrink-0" />
                        )}
                    </div>
                    <p className="text-sm text-mid-gray leading-relaxed">{description}</p>

                    {!granted && (
                        <div className="mt-4">
                            <button
                                onClick={handleGrant}
                                disabled={requesting}
                                className="
                  flex items-center gap-2
                  px-4 py-2 rounded-lg text-sm font-medium
                  bg-slider-fill text-white
                  hover:opacity-90 active:opacity-80
                  transition-opacity cursor-pointer
                  disabled:opacity-50 disabled:cursor-not-allowed
                "
                            >
                                {requesting ? (
                                    <>
                                        <Loader2 size={14} className="animate-spin" />
                                        Requesting…
                                    </>
                                ) : (
                                    "Grant Access"
                                )}
                            </button>
                        </div>
                    )}
                </div>
            </div>

            {/* Status badge */}
            <div className="absolute top-4 right-4">
                <span
                    className={`
            text-xs font-medium px-2.5 py-1 rounded-full
            ${granted
                            ? "bg-green-500/15 text-green-600 dark:text-green-400"
                            : "bg-amber-500/15 text-amber-600 dark:text-amber-400"
                        }
          `}
                >
                    {granted ? "Granted" : "Required"}
                </span>
            </div>
        </div>
    );
};

interface PermissionsOnboardingProps {
    onContinue: () => void;
}

export const PermissionsOnboarding: React.FC<PermissionsOnboardingProps> = ({ onContinue }) => {
    const { permissions, loading, allGranted, requestPermission } = usePermissions();

    if (loading && !permissions) {
        return (
            <div className="h-screen flex items-center justify-center bg-background">
                <div className="flex flex-col items-center gap-3">
                    <Loader2 size={32} className="text-slider-fill animate-spin" />
                    <span className="text-sm text-mid-gray">Checking permissions…</span>
                </div>
            </div>
        );
    }

    const micGranted = permissions?.microphone === "granted";
    const screenGranted = permissions?.screen_recording === true;

    return (
        <div className="h-screen flex flex-col select-none cursor-default bg-background text-text overflow-hidden">
            <div className="flex-1 flex items-center justify-center overflow-y-auto">
                <div className="w-full max-w-lg px-8 py-12 flex flex-col items-center gap-8">
                    {/* Header */}
                    <div className="flex flex-col items-center gap-4 text-center">
                        <div className="flex items-center justify-center w-16 h-16 rounded-2xl bg-slider-fill/15">
                            <Shield size={32} className="text-slider-fill" />
                        </div>
                        <div>
                            <h1 className="text-2xl font-bold text-text mb-2">
                                Permissions Required
                            </h1>
                            <p className="text-sm text-mid-gray leading-relaxed max-w-sm">
                                Crispy needs access to your microphone and screen recording to process audio and capture app sounds.
                            </p>
                        </div>
                    </div>

                    {/* Permission cards */}
                    <div className="w-full flex flex-col gap-4">
                        <PermissionCard
                            icon={Mic}
                            title="Microphone"
                            description="Required for real-time noise suppression and audio recording. All processing happens locally."
                            granted={micGranted}
                            onGrant={() => requestPermission("microphone").then(() => { })}
                        />

                        <PermissionCard
                            icon={Monitor}
                            title="Screen Recording"
                            description="Required to capture audio from other apps (Zoom, Chrome, etc.). Only audio is captured."
                            granted={screenGranted}
                            onGrant={() => requestPermission("screen_recording").then(() => { })}
                        />
                    </div>

                    {/* Footer */}
                    <div className="w-full flex flex-col items-center gap-3 pt-2">
                        {allGranted ? (
                            <button
                                onClick={onContinue}
                                className="
                  w-full py-3 rounded-xl text-sm font-semibold
                  bg-green-500 text-white
                  hover:bg-green-600 active:bg-green-700
                  transition-colors cursor-pointer
                  shadow-lg shadow-green-500/20
                "
                            >
                                Continue to Crispy ✨
                            </button>
                        ) : (
                            <p className="text-xs text-mid-gray/60 flex items-center gap-1.5">
                                <Loader2 size={12} className="animate-spin" />
                                Waiting for permissions…
                            </p>
                        )}

                        <button
                            onClick={onContinue}
                            className="text-xs text-mid-gray/60 hover:text-mid-gray transition-colors cursor-pointer"
                        >
                            Skip for now
                        </button>
                    </div>
                </div>
            </div>
        </div>
    );
};
