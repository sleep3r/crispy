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
    return stored ? parseFloat(stored) : 1.0;
  } catch {
    return 1.0;
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
  const [duration, setDuration] = useState(0);
  const [currentTime, setCurrentTime] = useState(0);
  const [isDragging, setIsDragging] = useState(false);
  const [volume, setVolume] = useState(getStoredVolume());
  const audioRef = useRef<HTMLAudioElement>(null);
  const animationRef = useRef<number>();
  const dragTimeRef = useRef<number>(0);

  // Use refs to avoid stale closures in animation loop
  const isPlayingRef = useRef(false);
  const isDraggingRef = useRef(false);

  // Keep refs in sync with state
  useEffect(() => {
    isPlayingRef.current = isPlaying;
  }, [isPlaying]);

  useEffect(() => {
    isDraggingRef.current = isDragging;
  }, [isDragging]);

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

  // Audio event handlers: metadata, ended, play/pause, and timeupdate for progress
  useEffect(() => {
    const audio = audioRef.current;
    if (!audio) return;

    const handleLoadedMetadata = () => {
      setDuration(audio.duration || 0);
      setCurrentTime(0);
    };

    const handleEnded = () => {
      setIsPlaying(false);
      onPlayStateChange(false);
      setCurrentTime(audio.duration || 0);
    };

    const handlePlay = () => {
      setIsPlaying(true);
      onPlayStateChange(true);
    };

    const handlePause = () => {
      setIsPlaying(false);
      onPlayStateChange(false);
    };

    const handleTimeUpdate = () => {
      if (!isDraggingRef.current) {
        setCurrentTime(audio.currentTime);
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
  }, [onPlayStateChange]);

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
        await audio.play();
      }
    } catch (error) {
      console.error("Playback failed:", error);
    }
  };

  const handleSeek = (e: React.ChangeEvent<HTMLInputElement>) => {
    const newTime = parseFloat(e.target.value);
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
    const newVolume = parseFloat(e.target.value);
    setVolume(newVolume);
    storeVolume(newVolume);
  };

  const toggleMute = () => {
    const newVolume = volume === 0 ? 1.0 : 0;
    setVolume(newVolume);
    storeVolume(newVolume);
  };

  const formatTime = (time: number): string => {
    if (!isFinite(time)) return "0:00";

    const minutes = Math.floor(time / 60);
    const seconds = Math.floor(time % 60);
    return `${minutes}:${seconds.toString().padStart(2, "0")}`;
  };

  // Single source of truth for seek position; avoid max=0 so range input thumb stays in sync
  const safeDuration = Math.max(duration, 0.01);
  const seekValue = Math.min(Math.max(0, currentTime), safeDuration);
  const progressPercent = duration <= 0 ? 0 : Math.min(100, (seekValue / duration) * 100);

  const VolumeIcon = volume === 0 ? VolumeX : volume < 0.5 ? Volume1 : Volume2;

  return (
    <div className={`flex items-center gap-3 ${className}`}>
      <audio ref={audioRef} src={src} preload="metadata" />

      {/* Play/Pause Button */}
      <button
        type="button"
        onClick={togglePlay}
        className="p-1 rounded hover:bg-mid-gray/10 transition-colors"
        aria-label={isPlaying ? "Pause" : "Play"}
      >
        {isPlaying ? (
          <Pause size={20} className="text-logo-primary" fill="currentColor" />
        ) : (
          <Play size={20} className="text-mid-gray" fill="currentColor" />
        )}
      </button>

      {/* Time and Seekbar */}
      <div className="flex-1 flex items-center gap-2">
        <span className="text-xs text-mid-gray min-w-[35px] tabular-nums">
          {formatTime(currentTime)}
        </span>

        <div className="relative flex-1 h-6">
          {/* Background track */}
          <div className="absolute inset-0 h-[4px] bg-mid-gray/20 rounded-full top-1/2 -translate-y-1/2" />

          {/* Filled track — same value as input so thumb and fill stay in sync */}
          <div
            className="absolute h-[4px] rounded-full top-1/2 -translate-y-1/2 pointer-events-none"
            style={{
              width: `${progressPercent}%`,
              backgroundColor: "var(--color-slider-fill)",
            }}
          />

          {/* Seek input — value and max from same source so thumb matches fill */}
          <input
            type="range"
            min={0}
            max={safeDuration}
            step="0.01"
            value={seekValue}
            onChange={handleSeek}
            onMouseDown={handleSeekMouseDown}
            onTouchStart={handleSeekTouchStart}
            className="relative w-full h-6 bg-transparent appearance-none cursor-pointer focus:outline-none slider-custom z-10"
          />
        </div>

        <span className="text-xs text-mid-gray min-w-[35px] tabular-nums">
          {formatTime(duration)}
        </span>
      </div>

      {/* Volume: icon + inline slider (same row as other controls) */}
      <div className="flex items-center gap-2 shrink-0">
        <button
          type="button"
          onClick={toggleMute}
          className="p-1 rounded hover:bg-mid-gray/10 transition-colors"
          aria-label={volume === 0 ? "Unmute" : "Mute"}
        >
          <VolumeIcon size={18} className="text-mid-gray" />
        </button>
        <div className="relative w-20 h-6">
          <div className="absolute inset-0 h-[4px] bg-mid-gray/20 rounded-full top-1/2 -translate-y-1/2" />
          <div
            className="absolute h-[4px] rounded-full top-1/2 -translate-y-1/2 transition-all duration-75 ease-out pointer-events-none"
            style={{
              width: `${volume * 100}%`,
              backgroundColor: "var(--color-slider-fill)",
            }}
          />
          <input
            type="range"
            min="0"
            max="1"
            step="0.01"
            value={volume}
            onChange={handleVolumeChange}
            className="relative w-full h-6 bg-transparent appearance-none cursor-pointer focus:outline-none slider-custom z-10"
          />
        </div>
      </div>
    </div>
  );
};
