/*
 * C header for Crispy Virtual Microphone shared memory IPC
 * 
 * This header defines the shared memory layout for communication
 * between the Crispy app and the AudioServerPlugIn.
 */

#ifndef CRISPY_VIRTUAL_MIC_H
#define CRISPY_VIRTUAL_MIC_H

#include <stdint.h>
#include <stdatomic.h>

#ifdef __cplusplus
extern "C" {
#endif

// Magic number to identify Crispy virtual mic shared memory
#define CRISPY_MAGIC 0x43525350  // "CRSP"

// Protocol version
#define PROTOCOL_VERSION 1

// Audio format constants
#define SAMPLE_RATE 48000
#define CHANNELS 1
#define SAMPLE_FORMAT 0  // 0 = Float32

// Buffer capacity in frames
#define CAPACITY_FRAMES 9600

// Shared memory name
#define SHM_NAME "/crispy_virtual_mic"

// Header structure at the start of shared memory
typedef struct {
    uint32_t magic;                    // Magic number for validation
    uint32_t version;                  // Protocol version
    uint32_t sample_rate;              // Sample rate in Hz
    uint32_t channels;                 // Number of channels
    uint32_t format;                   // Sample format (0 = Float32)
    uint32_t capacity_frames;          // Ring buffer capacity in frames
    
    // Atomic indices and counters
    _Atomic uint32_t write_index;      // Write position (in frames)
    _Atomic uint32_t read_index;       // Read position (in frames)
    _Atomic uint64_t underrun_count;   // Number of underruns
    _Atomic uint64_t overrun_count;    // Number of overruns
    _Atomic uint64_t sequence;         // Monotonic frame counter
} CrispyVirtualMicHeader;

// Calculate total shared memory size
static inline size_t crispy_shared_memory_size(void) {
    return sizeof(CrispyVirtualMicHeader) + (CAPACITY_FRAMES * CHANNELS * sizeof(float));
}

// Get pointer to ring buffer data (following header)
static inline float* crispy_get_buffer(void* shm_ptr) {
    return (float*)((uint8_t*)shm_ptr + sizeof(CrispyVirtualMicHeader));
}

// Validate header
static inline int crispy_validate_header(const CrispyVirtualMicHeader* header) {
    return header->magic == CRISPY_MAGIC && header->version == PROTOCOL_VERSION;
}

#ifdef __cplusplus
}
#endif

#endif // CRISPY_VIRTUAL_MIC_H
