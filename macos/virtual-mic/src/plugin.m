#import <Foundation/Foundation.h>
#import <CoreAudio/AudioServerPlugIn.h>
#import <mach/mach_time.h>
#import "plugin.h"

// Device and stream constants
#define kDevice_UID "CrispyVirtualMic_UID"
#define kDevice_ModelUID "CrispyVirtualMic_ModelUID"
#define kDevice_Name "Crispy Microphone"
#define kStream_Name "Crispy Microphone Stream"

// Object IDs
enum {
    kObjectID_PlugIn = kAudioObjectPlugInObject,
    kObjectID_Device = 2,
    kObjectID_Stream = 3,
};

// Sample rate and format
#define kSampleRate 48000.0
#define kChannels 1
#define kBitsPerChannel 32
#define kBytesPerFrame (kChannels * sizeof(Float32))

// Forward declarations
static OSStatus PlugIn_QueryInterface(void* self, REFIID iid, LPVOID* ppv);
static ULONG PlugIn_AddRef(void* self);
static ULONG PlugIn_Release(void* self);
static OSStatus PlugIn_Initialize(AudioServerPlugInDriverRef driver, AudioServerPlugInHostRef host);
static OSStatus PlugIn_CreateDevice(AudioServerPlugInDriverRef driver, CFDictionaryRef description, const AudioServerPlugInClientInfo* clientInfo, AudioObjectID* outDeviceObjectID);
static OSStatus PlugIn_DestroyDevice(AudioServerPlugInDriverRef driver, AudioObjectID deviceObjectID);
static OSStatus PlugIn_AddDeviceClient(AudioServerPlugInDriverRef driver, AudioObjectID deviceObjectID, const AudioServerPlugInClientInfo* clientInfo);
static OSStatus PlugIn_RemoveDeviceClient(AudioServerPlugInDriverRef driver, AudioObjectID deviceObjectID, const AudioServerPlugInClientInfo* clientInfo);
static OSStatus PlugIn_PerformDeviceConfigurationChange(AudioServerPlugInDriverRef driver, AudioObjectID deviceObjectID, UInt64 changeAction, void* changeInfo);
static OSStatus PlugIn_AbortDeviceConfigurationChange(AudioServerPlugInDriverRef driver, AudioObjectID deviceObjectID, UInt64 changeAction, void* changeInfo);
static Boolean PlugIn_HasProperty(AudioServerPlugInDriverRef driver, AudioObjectID objectID, pid_t clientProcessID, const AudioObjectPropertyAddress* address);
static OSStatus PlugIn_IsPropertySettable(AudioServerPlugInDriverRef driver, AudioObjectID objectID, pid_t clientProcessID, const AudioObjectPropertyAddress* address, Boolean* outIsSettable);
static OSStatus PlugIn_GetPropertyDataSize(AudioServerPlugInDriverRef driver, AudioObjectID objectID, pid_t clientProcessID, const AudioObjectPropertyAddress* address, UInt32 qualifierDataSize, const void* qualifierData, UInt32* outDataSize);
static OSStatus PlugIn_GetPropertyData(AudioServerPlugInDriverRef driver, AudioObjectID objectID, pid_t clientProcessID, const AudioObjectPropertyAddress* address, UInt32 qualifierDataSize, const void* qualifierData, UInt32 inDataSize, UInt32* outDataSize, void* outData);
static OSStatus PlugIn_SetPropertyData(AudioServerPlugInDriverRef driver, AudioObjectID objectID, pid_t clientProcessID, const AudioObjectPropertyAddress* address, UInt32 qualifierDataSize, const void* qualifierData, UInt32 inDataSize, const void* inData);
static OSStatus PlugIn_StartIO(AudioServerPlugInDriverRef driver, AudioObjectID deviceObjectID, UInt32 clientID);
static OSStatus PlugIn_StopIO(AudioServerPlugInDriverRef driver, AudioObjectID deviceObjectID, UInt32 clientID);
static OSStatus PlugIn_GetZeroTimeStamp(AudioServerPlugInDriverRef driver, AudioObjectID deviceObjectID, UInt32 clientID, Float64* outSampleTime, UInt64* outHostTime, UInt64* outSeed);
static OSStatus PlugIn_WillDoIOOperation(AudioServerPlugInDriverRef driver, AudioObjectID deviceObjectID, UInt32 clientID, UInt32 operationID, Boolean* outWillDo, Boolean* outWillDoInPlace);
static OSStatus PlugIn_BeginIOOperation(AudioServerPlugInDriverRef driver, AudioObjectID deviceObjectID, UInt32 clientID, UInt32 operationID, UInt32 ioBufferFrameSize, const AudioServerPlugInIOCycleInfo* ioCycleInfo);
static OSStatus PlugIn_DoIOOperation(AudioServerPlugInDriverRef driver, AudioObjectID deviceObjectID, AudioObjectID streamObjectID, UInt32 clientID, UInt32 operationID, UInt32 ioBufferFrameSize, const AudioServerPlugInIOCycleInfo* ioCycleInfo, void* ioMainBuffer, void* ioSecondaryBuffer);
static OSStatus PlugIn_EndIOOperation(AudioServerPlugInDriverRef driver, AudioObjectID deviceObjectID, UInt32 clientID, UInt32 operationID, UInt32 ioBufferFrameSize, const AudioServerPlugInIOCycleInfo* ioCycleInfo);

// Plugin state
typedef struct {
    AudioServerPlugInDriverInterface interface;
    AudioServerPlugInHostRef host;
    CFUUIDRef factoryUUID;
    UInt32 refCount;
    Boolean isIORunning;
    Float64 sampleTime;
    UInt64 hostTime;
} PlugInState;

static PlugInState* gPlugIn = NULL;

// VTable
static AudioServerPlugInDriverInterface gPlugInInterface = {
    NULL, // padding
    PlugIn_QueryInterface,
    PlugIn_AddRef,
    PlugIn_Release,
    PlugIn_Initialize,
    PlugIn_CreateDevice,
    PlugIn_DestroyDevice,
    PlugIn_AddDeviceClient,
    PlugIn_RemoveDeviceClient,
    PlugIn_PerformDeviceConfigurationChange,
    PlugIn_AbortDeviceConfigurationChange,
    PlugIn_HasProperty,
    PlugIn_IsPropertySettable,
    PlugIn_GetPropertyDataSize,
    PlugIn_GetPropertyData,
    PlugIn_SetPropertyData,
    PlugIn_StartIO,
    PlugIn_StopIO,
    PlugIn_GetZeroTimeStamp,
    PlugIn_WillDoIOOperation,
    PlugIn_BeginIOOperation,
    PlugIn_DoIOOperation,
    PlugIn_EndIOOperation
};

#pragma mark - Factory

void* CrispyVirtualMicPlugInFactory(CFAllocatorRef allocator, CFUUIDRef typeID) {
    if (!CFEqual(typeID, kAudioServerPlugInTypeUUID)) {
        return NULL;
    }
    
    if (gPlugIn != NULL) {
        gPlugIn->refCount++;
        return gPlugIn;
    }
    
    gPlugIn = (PlugInState*)calloc(1, sizeof(PlugInState));
    if (gPlugIn == NULL) {
        return NULL;
    }
    
    gPlugIn->interface = gPlugInInterface;
    gPlugIn->refCount = 1;
    gPlugIn->factoryUUID = CFUUIDGetConstantUUIDWithBytes(NULL,
        0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0,
        0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88);
    CFRetain(gPlugIn->factoryUUID);
    gPlugIn->isIORunning = false;
    gPlugIn->sampleTime = 0.0;
    gPlugIn->hostTime = mach_absolute_time();
    
    // Initialize shared memory
    crispy_init_shm();
    
    return gPlugIn;
}

#pragma mark - COM Methods

static OSStatus PlugIn_QueryInterface(void* self, REFIID iid, LPVOID* ppv) {
    PlugInState* plugin = (PlugInState*)self;
    CFUUIDRef interfaceID = CFUUIDCreateFromUUIDBytes(NULL, iid);
    
    if (CFEqual(interfaceID, kAudioServerPlugInDriverInterfaceUUID) ||
        CFEqual(interfaceID, IUnknownUUID)) {
        plugin->refCount++;
        *ppv = plugin;
        CFRelease(interfaceID);
        return kAudioHardwareNoError;
    }
    
    CFRelease(interfaceID);
    return E_NOINTERFACE;
}

static ULONG PlugIn_AddRef(void* self) {
    PlugInState* plugin = (PlugInState*)self;
    return ++plugin->refCount;
}

static ULONG PlugIn_Release(void* self) {
    PlugInState* plugin = (PlugInState*)self;
    UInt32 refCount = --plugin->refCount;
    
    if (refCount == 0) {
        crispy_cleanup_shm();
        if (plugin->factoryUUID) {
            CFRelease(plugin->factoryUUID);
        }
        free(plugin);
        gPlugIn = NULL;
    }
    
    return refCount;
}

#pragma mark - Plugin Methods

static OSStatus PlugIn_Initialize(AudioServerPlugInDriverRef driver, AudioServerPlugInHostRef host) {
    PlugInState* plugin = (PlugInState*)driver;
    plugin->host = host;
    return kAudioHardwareNoError;
}

static OSStatus PlugIn_CreateDevice(AudioServerPlugInDriverRef driver, CFDictionaryRef description, const AudioServerPlugInClientInfo* clientInfo, AudioObjectID* outDeviceObjectID) {
    *outDeviceObjectID = kObjectID_Device;
    return kAudioHardwareNoError;
}

static OSStatus PlugIn_DestroyDevice(AudioServerPlugInDriverRef driver, AudioObjectID deviceObjectID) {
    return kAudioHardwareNoError;
}

static OSStatus PlugIn_AddDeviceClient(AudioServerPlugInDriverRef driver, AudioObjectID deviceObjectID, const AudioServerPlugInClientInfo* clientInfo) {
    return kAudioHardwareNoError;
}

static OSStatus PlugIn_RemoveDeviceClient(AudioServerPlugInDriverRef driver, AudioObjectID deviceObjectID, const AudioServerPlugInClientInfo* clientInfo) {
    return kAudioHardwareNoError;
}

static OSStatus PlugIn_PerformDeviceConfigurationChange(AudioServerPlugInDriverRef driver, AudioObjectID deviceObjectID, UInt64 changeAction, void* changeInfo) {
    return kAudioHardwareNoError;
}

static OSStatus PlugIn_AbortDeviceConfigurationChange(AudioServerPlugInDriverRef driver, AudioObjectID deviceObjectID, UInt64 changeAction, void* changeInfo) {
    return kAudioHardwareNoError;
}

#pragma mark - Property Support

static Boolean PlugIn_HasProperty(AudioServerPlugInDriverRef driver, AudioObjectID objectID, pid_t clientProcessID, const AudioObjectPropertyAddress* address) {
    Boolean hasProperty = false;
    
    switch (objectID) {
        case kObjectID_PlugIn:
            switch (address->mSelector) {
                case kAudioObjectPropertyManufacturer:
                case kAudioObjectPropertyOwnedObjects:
                case kAudioPlugInPropertyDeviceList:
                case kAudioPlugInPropertyTranslateUIDToDevice:
                    hasProperty = true;
                    break;
            }
            break;
            
        case kObjectID_Device:
            switch (address->mSelector) {
                case kAudioObjectPropertyName:
                case kAudioObjectPropertyManufacturer:
                case kAudioObjectPropertyOwnedObjects:
                case kAudioDevicePropertyDeviceUID:
                case kAudioDevicePropertyModelUID:
                case kAudioDevicePropertyTransportType:
                case kAudioDevicePropertyStreams:
                case kAudioDevicePropertyNominalSampleRate:
                case kAudioDevicePropertyAvailableNominalSampleRates:
                case kAudioDevicePropertyIsHidden:
                case kAudioDevicePropertyZeroTimeStampPeriod:
                case kAudioDevicePropertyDeviceIsAlive:
                case kAudioDevicePropertyDeviceIsRunning:
                case kAudioDevicePropertyLatency:
                case kAudioDevicePropertySafetyOffset:
                    hasProperty = true;
                    break;
            }
            break;
            
        case kObjectID_Stream:
            switch (address->mSelector) {
                case kAudioObjectPropertyName:
                case kAudioStreamPropertyDirection:
                case kAudioStreamPropertyVirtualFormat:
                case kAudioStreamPropertyPhysicalFormat:
                case kAudioStreamPropertyAvailableVirtualFormats:
                case kAudioStreamPropertyAvailablePhysicalFormats:
                    hasProperty = true;
                    break;
            }
            break;
    }
    
    return hasProperty;
}

static OSStatus PlugIn_IsPropertySettable(AudioServerPlugInDriverRef driver, AudioObjectID objectID, pid_t clientProcessID, const AudioObjectPropertyAddress* address, Boolean* outIsSettable) {
    *outIsSettable = false;
    return kAudioHardwareNoError;
}

static OSStatus PlugIn_GetPropertyDataSize(AudioServerPlugInDriverRef driver, AudioObjectID objectID, pid_t clientProcessID, const AudioObjectPropertyAddress* address, UInt32 qualifierDataSize, const void* qualifierData, UInt32* outDataSize) {
    switch (objectID) {
        case kObjectID_PlugIn:
            switch (address->mSelector) {
                case kAudioObjectPropertyManufacturer:
                    *outDataSize = sizeof(CFStringRef);
                    return kAudioHardwareNoError;
                case kAudioObjectPropertyOwnedObjects:
                case kAudioPlugInPropertyDeviceList:
                    *outDataSize = sizeof(AudioObjectID);
                    return kAudioHardwareNoError;
                case kAudioPlugInPropertyTranslateUIDToDevice:
                    *outDataSize = sizeof(AudioObjectID);
                    return kAudioHardwareNoError;
            }
            break;
            
        case kObjectID_Device:
            switch (address->mSelector) {
                case kAudioObjectPropertyName:
                case kAudioObjectPropertyManufacturer:
                case kAudioDevicePropertyDeviceUID:
                case kAudioDevicePropertyModelUID:
                    *outDataSize = sizeof(CFStringRef);
                    return kAudioHardwareNoError;
                case kAudioObjectPropertyOwnedObjects:
                    *outDataSize = sizeof(AudioObjectID);
                    return kAudioHardwareNoError;
                case kAudioDevicePropertyTransportType:
                case kAudioDevicePropertyIsHidden:
                case kAudioDevicePropertyZeroTimeStampPeriod:
                case kAudioDevicePropertyDeviceIsAlive:
                case kAudioDevicePropertyDeviceIsRunning:
                case kAudioDevicePropertyLatency:
                case kAudioDevicePropertySafetyOffset:
                    *outDataSize = sizeof(UInt32);
                    return kAudioHardwareNoError;
                case kAudioDevicePropertyStreams:
                    // Return stream object IDs (scope-sensitive)
                    if (address->mScope == kAudioObjectPropertyScopeInput) {
                        *outDataSize = sizeof(AudioObjectID); // 1 input stream
                    } else {
                        *outDataSize = 0; // no output streams
                    }
                    return kAudioHardwareNoError;
                case kAudioDevicePropertyNominalSampleRate:
                    *outDataSize = sizeof(Float64);
                    return kAudioHardwareNoError;
                case kAudioDevicePropertyAvailableNominalSampleRates:
                    *outDataSize = sizeof(AudioValueRange);
                    return kAudioHardwareNoError;
            }
            break;
            
        case kObjectID_Stream:
            switch (address->mSelector) {
                case kAudioObjectPropertyName:
                    *outDataSize = sizeof(CFStringRef);
                    return kAudioHardwareNoError;
                case kAudioStreamPropertyDirection:
                    *outDataSize = sizeof(UInt32);
                    return kAudioHardwareNoError;
                case kAudioStreamPropertyVirtualFormat:
                case kAudioStreamPropertyPhysicalFormat:
                    *outDataSize = sizeof(AudioStreamBasicDescription);
                    return kAudioHardwareNoError;
                case kAudioStreamPropertyAvailableVirtualFormats:
                case kAudioStreamPropertyAvailablePhysicalFormats:
                    *outDataSize = sizeof(AudioStreamRangedDescription);
                    return kAudioHardwareNoError;
            }
            break;
    }
    
    return kAudioHardwareUnknownPropertyError;
}

static OSStatus PlugIn_GetPropertyData(AudioServerPlugInDriverRef driver, AudioObjectID objectID, pid_t clientProcessID, const AudioObjectPropertyAddress* address, UInt32 qualifierDataSize, const void* qualifierData, UInt32 inDataSize, UInt32* outDataSize, void* outData) {
    switch (objectID) {
        case kObjectID_PlugIn:
            switch (address->mSelector) {
                case kAudioObjectPropertyManufacturer:
                    *outDataSize = sizeof(CFStringRef);
                    *((CFStringRef*)outData) = CFSTR("Crispy");
                    return kAudioHardwareNoError;
                case kAudioObjectPropertyOwnedObjects:
                case kAudioPlugInPropertyDeviceList:
                    *outDataSize = sizeof(AudioObjectID);
                    ((AudioObjectID*)outData)[0] = kObjectID_Device;
                    return kAudioHardwareNoError;
                case kAudioPlugInPropertyTranslateUIDToDevice:
                    if (qualifierDataSize == sizeof(CFStringRef)) {
                        CFStringRef uid = *((CFStringRef*)qualifierData);
                        if (CFStringCompare(uid, CFSTR(kDevice_UID), 0) == kCFCompareEqualTo) {
                            *outDataSize = sizeof(AudioObjectID);
                            *((AudioObjectID*)outData) = kObjectID_Device;
                            return kAudioHardwareNoError;
                        }
                    }
                    return kAudioHardwareBadObjectError;
            }
            break;
            
        case kObjectID_Device:
            switch (address->mSelector) {
                case kAudioObjectPropertyName:
                    *outDataSize = sizeof(CFStringRef);
                    *((CFStringRef*)outData) = CFSTR(kDevice_Name);
                    return kAudioHardwareNoError;
                case kAudioObjectPropertyManufacturer:
                    *outDataSize = sizeof(CFStringRef);
                    *((CFStringRef*)outData) = CFSTR("Crispy");
                    return kAudioHardwareNoError;
                case kAudioDevicePropertyDeviceUID:
                    *outDataSize = sizeof(CFStringRef);
                    *((CFStringRef*)outData) = CFSTR(kDevice_UID);
                    return kAudioHardwareNoError;
                case kAudioDevicePropertyModelUID:
                    *outDataSize = sizeof(CFStringRef);
                    *((CFStringRef*)outData) = CFSTR(kDevice_ModelUID);
                    return kAudioHardwareNoError;
                case kAudioObjectPropertyOwnedObjects:
                    *outDataSize = sizeof(AudioObjectID);
                    ((AudioObjectID*)outData)[0] = kObjectID_Stream;
                    return kAudioHardwareNoError;
                case kAudioDevicePropertyTransportType:
                    *outDataSize = sizeof(UInt32);
                    *((UInt32*)outData) = kAudioDeviceTransportTypeVirtual;
                    return kAudioHardwareNoError;
                case kAudioDevicePropertyStreams:
                    // Return stream object IDs (scope-sensitive)
                    if (address->mScope == kAudioObjectPropertyScopeInput) {
                        *outDataSize = sizeof(AudioObjectID);
                        ((AudioObjectID*)outData)[0] = kObjectID_Stream;
                    } else {
                        *outDataSize = 0; // no output streams
                    }
                    return kAudioHardwareNoError;
                case kAudioDevicePropertyDeviceIsAlive:
                    *outDataSize = sizeof(UInt32);
                    *((UInt32*)outData) = 1;
                    return kAudioHardwareNoError;
                case kAudioDevicePropertyDeviceIsRunning:
                    *outDataSize = sizeof(UInt32);
                    *((UInt32*)outData) = gPlugIn ? gPlugIn->isIORunning : 0;
                    return kAudioHardwareNoError;
                case kAudioDevicePropertyLatency:
                case kAudioDevicePropertySafetyOffset:
                    *outDataSize = sizeof(UInt32);
                    *((UInt32*)outData) = 0;
                    return kAudioHardwareNoError;
                case kAudioDevicePropertyNominalSampleRate:
                    *outDataSize = sizeof(Float64);
                    *((Float64*)outData) = kSampleRate;
                    return kAudioHardwareNoError;
                case kAudioDevicePropertyAvailableNominalSampleRates:
                    *outDataSize = sizeof(AudioValueRange);
                    ((AudioValueRange*)outData)->mMinimum = kSampleRate;
                    ((AudioValueRange*)outData)->mMaximum = kSampleRate;
                    return kAudioHardwareNoError;
                case kAudioDevicePropertyIsHidden:
                    *outDataSize = sizeof(UInt32);
                    *((UInt32*)outData) = 0;
                    return kAudioHardwareNoError;
                case kAudioDevicePropertyZeroTimeStampPeriod:
                    *outDataSize = sizeof(UInt32);
                    *((UInt32*)outData) = 0;
                    return kAudioHardwareNoError;
            }
            break;
            
        case kObjectID_Stream:
            switch (address->mSelector) {
                case kAudioObjectPropertyName:
                    *outDataSize = sizeof(CFStringRef);
                    *((CFStringRef*)outData) = CFSTR(kStream_Name);
                    return kAudioHardwareNoError;
                case kAudioStreamPropertyDirection:
                    *outDataSize = sizeof(UInt32);
                    *((UInt32*)outData) = 1; // Input
                    return kAudioHardwareNoError;
                case kAudioStreamPropertyVirtualFormat:
                case kAudioStreamPropertyPhysicalFormat: {
                    AudioStreamBasicDescription* asbd = (AudioStreamBasicDescription*)outData;
                    *outDataSize = sizeof(AudioStreamBasicDescription);
                    asbd->mSampleRate = kSampleRate;
                    asbd->mFormatID = kAudioFormatLinearPCM;
                    asbd->mFormatFlags = kAudioFormatFlagIsFloat | kAudioFormatFlagIsPacked;
                    asbd->mBytesPerPacket = kBytesPerFrame;
                    asbd->mFramesPerPacket = 1;
                    asbd->mBytesPerFrame = kBytesPerFrame;
                    asbd->mChannelsPerFrame = kChannels;
                    asbd->mBitsPerChannel = kBitsPerChannel;
                    return kAudioHardwareNoError;
                }
                case kAudioStreamPropertyAvailableVirtualFormats:
                case kAudioStreamPropertyAvailablePhysicalFormats: {
                    AudioStreamRangedDescription* ranged = (AudioStreamRangedDescription*)outData;
                    *outDataSize = sizeof(AudioStreamRangedDescription);
                    ranged->mFormat.mSampleRate = kSampleRate;
                    ranged->mFormat.mFormatID = kAudioFormatLinearPCM;
                    ranged->mFormat.mFormatFlags = kAudioFormatFlagIsFloat | kAudioFormatFlagIsPacked;
                    ranged->mFormat.mBytesPerPacket = kBytesPerFrame;
                    ranged->mFormat.mFramesPerPacket = 1;
                    ranged->mFormat.mBytesPerFrame = kBytesPerFrame;
                    ranged->mFormat.mChannelsPerFrame = kChannels;
                    ranged->mFormat.mBitsPerChannel = kBitsPerChannel;
                    ranged->mSampleRateRange.mMinimum = kSampleRate;
                    ranged->mSampleRateRange.mMaximum = kSampleRate;
                    return kAudioHardwareNoError;
                }
            }
            break;
    }
    
    return kAudioHardwareUnknownPropertyError;
}

static OSStatus PlugIn_SetPropertyData(AudioServerPlugInDriverRef driver, AudioObjectID objectID, pid_t clientProcessID, const AudioObjectPropertyAddress* address, UInt32 qualifierDataSize, const void* qualifierData, UInt32 inDataSize, const void* inData) {
    return kAudioHardwareUnsupportedOperationError;
}

#pragma mark - IO Operations

static OSStatus PlugIn_StartIO(AudioServerPlugInDriverRef driver, AudioObjectID deviceObjectID, UInt32 clientID) {
    PlugInState* plugin = (PlugInState*)driver;
    plugin->isIORunning = true;
    return kAudioHardwareNoError;
}

static OSStatus PlugIn_StopIO(AudioServerPlugInDriverRef driver, AudioObjectID deviceObjectID, UInt32 clientID) {
    PlugInState* plugin = (PlugInState*)driver;
    plugin->isIORunning = false;
    return kAudioHardwareNoError;
}

static OSStatus PlugIn_GetZeroTimeStamp(AudioServerPlugInDriverRef driver, AudioObjectID deviceObjectID, UInt32 clientID, Float64* outSampleTime, UInt64* outHostTime, UInt64* outSeed) {
    PlugInState* plugin = (PlugInState*)driver;
    *outSampleTime = plugin->sampleTime;
    *outHostTime = plugin->hostTime;
    *outSeed = 1;
    return kAudioHardwareNoError;
}

static OSStatus PlugIn_WillDoIOOperation(AudioServerPlugInDriverRef driver, AudioObjectID deviceObjectID, UInt32 clientID, UInt32 operationID, Boolean* outWillDo, Boolean* outWillDoInPlace) {
    *outWillDo = (operationID == kAudioServerPlugInIOOperationReadInput);
    *outWillDoInPlace = true;
    return kAudioHardwareNoError;
}

static OSStatus PlugIn_BeginIOOperation(AudioServerPlugInDriverRef driver, AudioObjectID deviceObjectID, UInt32 clientID, UInt32 operationID, UInt32 ioBufferFrameSize, const AudioServerPlugInIOCycleInfo* ioCycleInfo) {
    return kAudioHardwareNoError;
}

static OSStatus PlugIn_DoIOOperation(AudioServerPlugInDriverRef driver, AudioObjectID deviceObjectID, AudioObjectID streamObjectID, UInt32 clientID, UInt32 operationID, UInt32 ioBufferFrameSize, const AudioServerPlugInIOCycleInfo* ioCycleInfo, void* ioMainBuffer, void* ioSecondaryBuffer) {
    if (operationID != kAudioServerPlugInIOOperationReadInput || streamObjectID != kObjectID_Stream) {
        return kAudioHardwareNoError;
    }
    
    // Read from shared memory ring buffer
    Float32* buffer = (Float32*)ioMainBuffer;
    crispy_read_frames(buffer, ioBufferFrameSize);
    
    return kAudioHardwareNoError;
}

static OSStatus PlugIn_EndIOOperation(AudioServerPlugInDriverRef driver, AudioObjectID deviceObjectID, UInt32 clientID, UInt32 operationID, UInt32 ioBufferFrameSize, const AudioServerPlugInIOCycleInfo* ioCycleInfo) {
    PlugInState* plugin = (PlugInState*)driver;
    
    // Update timestamps
    plugin->sampleTime += ioBufferFrameSize;
    plugin->hostTime = mach_absolute_time();
    
    return kAudioHardwareNoError;
}
