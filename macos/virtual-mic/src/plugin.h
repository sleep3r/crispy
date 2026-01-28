/*
 * Crispy Virtual Microphone - AudioServerPlugIn C interface
 */

#ifndef CRISPY_PLUGIN_H
#define CRISPY_PLUGIN_H

#include <stdint.h>
#include <CoreAudio/AudioServerPlugIn.h>

#ifdef __cplusplus
extern "C" {
#endif

// Rust core functions (exported from static lib)
extern int32_t crispy_init_shm(void);
extern void crispy_cleanup_shm(void);
extern int32_t crispy_is_shm_available(void);
extern uint32_t crispy_read_frames(float* buffer, uint32_t frame_count);
extern uint32_t crispy_get_fill_level(void);
extern uint64_t crispy_get_underrun_count(void);
extern uint64_t crispy_get_overrun_count(void);
extern uint32_t crispy_get_read_index(void);
extern uint32_t crispy_get_write_index(void);

// Plugin entry point
extern void* CrispyVirtualMicPlugInFactory(CFAllocatorRef allocator, CFUUIDRef typeID);

#ifdef __cplusplus
}
#endif

#endif // CRISPY_PLUGIN_H
