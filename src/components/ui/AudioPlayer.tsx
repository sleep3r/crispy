import React, { useState, useRef, useEffect, useCallback } from "react";
import { Play, Pause, Volume2, VolumeX, Volume1 } from "lucide-react";

interface AudioPlayerProps {
  src: string;
  isActive: boolean;
  onPlayStateChange: (playing: boolean) => void;
  className?: string;
}

const VOLUME_STORAGE_KEY = "crispy_audio_volume";

const getStoredVolume = (): number => {
  try {
    const stored = localStorage.getItem(VOLUME_STORAGE_KEY);
    return stored ? Number.parseFloat(stored) : 1;
  } catch {
    return 1;
  }
};

const storeVolume = (volume: number) => {
  try {
    localStorage.setItem(VOLUME_STORAGE_KEY, volume.toString());
  } catch {
    // Ignore localStorage errors
  }
};

export const AudioPlayer: React.FC<AudioPlayerProps> = ({
  src,
  isActive,
  onPlayStateChange,
  className = "",
}) => {
  const [isPlaying, setIsPlaying] = useState(false);
  const [duration, setDuration] = useState<number | null>(null);
  const [currentTime, setCurrentTime] = useState(0);
  const [isDragging, setIsDragging] = useState(false);
  const [volume, setVolume] = useState(getStoredVolume());
  const audioRef = useRef<HTMLAudioElement>(null);
  const animationRef = useRef<number>();
  const dragTimeRef = useRef<number>(0);

  // Use refs to avoid stale closures
  const isPlayingRef = useRef(false);
  const isDraggingRef = useRef(false);
  const onPlayStateChangeRef = useRef(onPlayStateChange);

  // Keep refs in sync
  useEffect(() => {
    isPlayingRef.current = isPlaying;
  }, [isPlaying]);

  useEffect(() => {
    isDraggingRef.current = isDragging;
  }, [isDragging]);

  useEffect(() => {
    onPlayStateChangeRef.current = onPlayStateChange;
  }, [onPlayStateChange]);

  // Auto-pause when another player becomes active
  useEffect(() => {
    if (!isActive && isPlaying && audioRef.current) {
      audioRef.current.pause();
    }
  }, [isActive, isPlaying]);

  // Update audio volume
  useEffect(() => {
    if (audioRef.current) {
      audioRef.current.volume = volume;
    }
  }, [volume]);

  // Stable animation loop for smooth progress during playback
  const tick = useCallback(() => {
    if (audioRef.current && !isDraggingRef.current) {
      setCurrentTime(audioRef.current.currentTime);
    }
    if (isPlayingRef.current) {
      animationRef.current = requestAnimationFrame(tick);
    }
  }, []);

  // Start/stop animation loop when playing and not dragging
  useEffect(() => {
    if (isPlaying && !isDragging) {
      animationRef.current = requestAnimationFrame(tick);
    } else if (animationRef.current) {
      cancelAnimationFrame(animationRef.current);
      animationRef.current = undefined;
    }
    return () => {
      if (animationRef.current) {
        cancelAnimationFrame(animationRef.current);
        animationRef.current = undefined;
      }
    };
  }, [isPlaying, isDragging, tick]);

  // Audio event handlers - setup once, never re-run (use refs for callbacks)
  useEffect(() => {
    const audio = audioRef.current;
    if (!audio) return;

    const handleLoadedMetadata = () => {
      const d = audio.duration;
      if (Number.isFinite(d) && d > 0) {
        setDuration(d);
      }
    };

    const handleEnded = () => {
      setIsPlaying(false);
      onPlayStateChangeRef.current(false);
      if (audio.duration && Number.isFinite(audio.duration)) {
        setCurrentTime(audio.duration);
      }
    };

    const handlePlay = () => {
      setIsPlaying(true);
      onPlayStateChangeRef.current(true);
    };

    const handlePause = () => {
      setIsPlaying(false);
      onPlayStateChangeRef.current(false);
      // Sync currentTime on pause (don't reset to 0!)
      if (!isDraggingRef.current) {
        setCurrentTime(audio.currentTime);
      }
    };

    const handleTimeUpdate = () => {
      if (!isDraggingRef.current) {
        setCurrentTime(audio.currentTime);
      }
      // Also try to pick up duration if it becomes available during playback
      if (duration == null) {
        const d = audio.duration;
        if (Number.isFinite(d) && d > 0) {
          setDuration(d);
        }
      }
    };

    audio.addEventListener("loadedmetadata", handleLoadedMetadata);
    audio.addEventListener("ended", handleEnded);
    audio.addEventListener("play", handlePlay);
    audio.addEventListener("pause", handlePause);
    audio.addEventListener("timeupdate", handleTimeUpdate);

    return () => {
      audio.removeEventListener("loadedmetadata", handleLoadedMetadata);
      audio.removeEventListener("ended", handleEnded);
      audio.removeEventListener("play", handlePlay);
      audio.removeEventListener("pause", handlePause);
      audio.removeEventListener("timeupdate", handleTimeUpdate);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []); // Empty deps - setup once, use refs for everything

  // Global drag handlers
  const handleMouseUp = useCallback(() => {
    if (isDragging) {
      setIsDragging(false);
      if (audioRef.current) {
        audioRef.current.currentTime = dragTimeRef.current;
        setCurrentTime(dragTimeRef.current);
      }
    }
  }, [isDragging]);

  useEffect(() => {
    if (isDragging) {
      document.addEventListener("mouseup", handleMouseUp);
      document.addEventListener("touchend", handleMouseUp);

      return () => {
        document.removeEventListener("mouseup", handleMouseUp);
        document.removeEventListener("touchend", handleMouseUp);
      };
    }
  }, [isDragging, handleMouseUp]);

  const togglePlay = async () => {
    const audio = audioRef.current;
    if (!audio) return;

    try {
      if (isPlaying) {
        audio.pause();
      } else {
        // If audio hasn't loaded yet (preload="none"), trigger load first
        if (audio.readyState < 1) {
          audio.load();
          await new Promise<void>((resolve, reject) => {
            const onLoaded = () => {
              audio.removeEventListener("loadedmetadata", onLoaded);
              audio.removeEventListener("error", onError);
              resolve();
            };
            const onError = () => {
              audio.removeEventListener("loadedmetadata", onLoaded);
              audio.removeEventListener("error", onError);
              reject(new Error("Failed to load audio"));
            };
            audio.addEventListener("loadedmetadata", onLoaded, { once: true });
            audio.addEventListener("error", onError, { once: true });
          });
        }
        await audio.play();
      }
    } catch (error) {
      // Ignore AbortError (happens when audio source changes during load)
      if (error instanceof DOMException && error.name === "AbortError") return;
      console.error("Playback failed:", error);
    }
  };

  const handleSeek = (e: React.ChangeEvent<HTMLInputElement>) => {
    const newTime = Number.parseFloat(e.target.value);
    dragTimeRef.current = newTime;
    setCurrentTime(newTime);

    if (!isDragging && audioRef.current) {
      audioRef.current.currentTime = newTime;
    }
  };

  const handleSeekMouseDown = () => {
    setIsDragging(true);
  };

  const handleSeekTouchStart = () => {
    setIsDragging(true);
  };

  const handleVolumeChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const newVolume = Number.parseFloat(e.target.value);
    setVolume(newVolume);
    storeVolume(newVolume);
  };

  const toggleMute = () => {
    const newVolume = volume === 0 ? 1 : 0;
    setVolume(newVolume);
    storeVolume(newVolume);
  };

  const formatTime = (time: number | null): string => {
    if (time == null || !Number.isFinite(time)) return "—:—";

    const mins = Math.floor(time / 60);
    const secs = Math.floor(time % 60);
    return `${mins}:${secs.toString().padStart(2, "0")}`;
  };

  const VolumeIcon = volume === 0 ? VolumeX : volume < 0.5 ? Volume1 : Volume2;

  return (
    <div className={`flex items-center gap-3 ${className}`}>
      {/* Use preload="none" to avoid metadata fetch storm on list mount */}
      <audio ref={audioRef} src={src} preload="none" />

      <button
        type="button"
        onClick={togglePlay}
        className="flex-shrink-0 w-8 h-8 flex items-center justify-center rounded-full bg-blue-500 hover:bg-blue-600 text-white transition-colors"
        aria-label={isPlaying ? "Pause" : "Play"}
      >
        {isPlaying ? <Pause className="w-4 h-4" /> : <Play className="w-4 h-4 ml-0.5" />}
      </button>

      <div className="flex-1 flex items-center gap-2">
        <span className="text-xs text-mid-gray font-mono w-10 text-right">
          {formatTime(currentTime)}
        </span>
        <input
          type="range"
          min="0"
          max={duration || 100}
          value={currentTime}
          onChange={handleSeek}
          onMouseDown={handleSeekMouseDown}
          onTouchStart={handleSeekTouchStart}
          className="flex-1 h-1.5 bg-mid-gray/20 rounded-full appearance-none cursor-pointer [&::-webkit-slider-thumb]:appearance-none [&::-webkit-slider-thumb]:w-3 [&::-webkit-slider-thumb]:h-3 [&::-webkit-slider-thumb]:rounded-full [&::-webkit-slider-thumb]:bg-blue-500 [&::-webkit-slider-thumb]:cursor-pointer [&::-moz-range-thumb]:w-3 [&::-moz-range-thumb]:h-3 [&::-moz-range-thumb]:rounded-full [&::-moz-range-thumb]:bg-blue-500 [&::-moz-range-thumb]:border-0 [&::-moz-range-thumb]:cursor-pointer"
          disabled={!duration}
        />
        <span className="text-xs text-mid-gray font-mono w-10">
          {formatTime(duration)}
        </span>
      </div>

      <div className="flex items-center gap-2">
        <button
          type="button"
          onClick={toggleMute}
          className="flex-shrink-0 p-1.5 rounded hover:bg-mid-gray/10 transition-colors"
          aria-label={volume === 0 ? "Unmute" : "Mute"}
        >
          <VolumeIcon className="w-4 h-4 text-mid-gray" />
        </button>
        <input
          type="range"
          min="0"
          max="1"
          step="0.01"
          value={volume}
          onChange={handleVolumeChange}
          className="w-20 h-1.5 bg-mid-gray/20 rounded-full appearance-none cursor-pointer [&::-webkit-slider-thumb]:appearance-none [&::-webkit-slider-thumb]:w-3 [&::-webkit-slider-thumb]:h-3 [&::-webkit-slider-thumb]:rounded-full [&::-webkit-slider-thumb]:bg-mid-gray [&::-webkit-slider-thumb]:cursor-pointer [&::-moz-range-thumb]:w-3 [&::-moz-range-thumb]:h-3 [&::-moz-range-thumb]:rounded-full [&::-moz-range-thumb]:bg-mid-gray [&::-moz-range-thumb]:border-0 [&::-moz-range-thumb]:cursor-pointer"
        />
      </div>
    </div>
  );
};
