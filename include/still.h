/* Generated from still-ffi. Do not edit by hand. */

#ifndef STILL_H
#define STILL_H

#pragma once

#include <stdarg.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>

#define STILL_ABI_VERSION 2

enum StillResult
#if __STDC_VERSION__ >= 202311L
  : int32_t
#endif // __STDC_VERSION__ >= 202311L
 {
  Ok = 0,
  InvalidArgument = 1,
  InvalidState = 2,
  Renderer = 3,
  BufferTooSmall = 4,
  Panic = 5,
  NotFound = 6,
};
#if __STDC_VERSION__ >= 202311L
typedef enum StillResult StillResult;
#else
typedef int32_t StillResult;
#endif // __STDC_VERSION__ >= 202311L

typedef struct StillEngine StillEngine;

/**
 * Generic transient scene overlay; the caller owns its time base and origin.
 */
typedef struct StillModulationFrame {
  uint32_t abi_version;
  uint32_t struct_size;
  double time_seconds;
  float motion;
  float turbulence;
  float light;
  uint32_t reserved;
} StillModulationFrame;

/**
 * Latest media palette, in normalized sRGB. This never changes saved presets.
 * Each color is RGB; energy is a normalized analyzed envelope, not audio PCM.
 */
typedef struct StillAmbientFrame {
  uint32_t abi_version;
  uint32_t struct_size;
  float color1[3];
  float color2[3];
  float color3[3];
  float energy;
} StillAmbientFrame;

typedef struct StillAmbientLightControl {
  float color[3];
  float gain;
  float offset[2];
  float scale[2];
  float rotation_degrees;
} StillAmbientLightControl;

/**
 * General light state, independent of how a caller derives its values.
 */
typedef struct StillAmbientControlFrame {
  uint32_t abi_version;
  uint32_t struct_size;
  struct StillAmbientLightControl lights[3];
  float brightness;
} StillAmbientControlFrame;

/**
 * Code-only timing in seconds. Zero disables smoothing for that property.
 */
typedef struct StillAmbientSmoothing {
  uint32_t abi_version;
  uint32_t struct_size;
  float color_seconds;
  float attack_seconds;
  float release_seconds;
  float transform_seconds;
} StillAmbientSmoothing;

typedef struct StillEngineConfig {
  uint32_t abi_version;
  uint32_t struct_size;
  uint32_t quality;
  uint32_t reserved;
} StillEngineConfig;

typedef struct StillPointerEvent {
  uint32_t abi_version;
  uint32_t struct_size;
  float x;
  float y;
  float velocity_x;
  float velocity_y;
  uint32_t active;
  uint32_t reserved;
} StillPointerEvent;

typedef struct StillParameterDescriptor {
  uint32_t abi_version;
  uint32_t struct_size;
  const char *stable_id;
  const char *display_name;
  float minimum;
  float maximum;
  float default_value;
  float smoothing_seconds;
} StillParameterDescriptor;

typedef struct StillSceneDescriptor {
  uint32_t abi_version;
  uint32_t struct_size;
  const char *stable_id;
  const char *display_name;
  uint32_t schema_version;
  uint32_t parameter_count;
} StillSceneDescriptor;

uint32_t still_abi_version(void);

/**
 * Replace a transient visual overlay on the engine's serial owner. Null clears
 * immediately; invalid input is rejected atomically. Saved parameters are untouched.
 */
StillResult still_engine_modulation_update(struct StillEngine *engine,
                                           const struct StillModulationFrame *frame);

/**
 * Atomically replace the transient palette/envelope. A null frame clears it
 * and fades back to manual colors. Call on the same serial queue as the engine.
 */
StillResult still_engine_ambient_update(struct StillEngine *engine,
                                        const struct StillAmbientFrame *frame);

StillResult still_engine_ambient_controls_update(struct StillEngine *engine,
                                                 const struct StillAmbientControlFrame *frame);

/**
 * Set external response times without changing manual scene controls or state.
 * Null resets the application-friendly defaults. Invalid settings are atomic.
 */
StillResult still_engine_ambient_smoothing_set(struct StillEngine *engine,
                                               const struct StillAmbientSmoothing *settings);

StillResult still_engine_create(const struct StillEngineConfig *config,
                                struct StillEngine **out_engine);

void still_engine_destroy(struct StillEngine *engine);

/**
 * Attach a retained AppKit NSView with a CAMetalLayer on macOS.
 * This entry point does not accept HWND, IInspectable or SwapChainPanel pointers.
 * Windows panel interop has a separate planned contract; see docs/windows/interop.md.
 */
StillResult still_engine_surface_attach(struct StillEngine *engine,
                                        void *ns_view,
                                        uint32_t width,
                                        uint32_t height);

StillResult still_engine_surface_detach(struct StillEngine *engine);

StillResult still_engine_resize(struct StillEngine *engine, uint32_t width, uint32_t height);

StillResult still_engine_suspend(struct StillEngine *engine);

StillResult still_engine_resume(struct StillEngine *engine);

StillResult still_engine_frame_tick(struct StillEngine *engine, double timestamp_seconds);

/**
 * Continuous display frames preserve palette interpolation across resume.
 * Use the ordinary frame tick for a single inactive-window paint.
 */
StillResult still_engine_frame_tick_animated(struct StillEngine *engine, double timestamp_seconds);

StillResult still_engine_input_batch(struct StillEngine *engine,
                                     const struct StillPointerEvent *events,
                                     size_t count);

StillResult still_engine_parameter_set(struct StillEngine *engine,
                                       const char *stable_id,
                                       float value);

StillResult still_engine_parameter_get(struct StillEngine *engine,
                                       const char *stable_id,
                                       float *out_value);

size_t still_parameter_count(void);

StillResult still_parameter_descriptor(size_t index, struct StillParameterDescriptor *out);

size_t still_scene_count(void);

StillResult still_scene_descriptor(size_t index, struct StillSceneDescriptor *out);

StillResult still_engine_scene_select(struct StillEngine *engine, const char *stable_id);

StillResult still_engine_scene_catalog_export(struct StillEngine *engine,
                                              uint8_t *buffer,
                                              size_t capacity,
                                              size_t *out_length);

StillResult still_engine_parameter_catalog_export(struct StillEngine *engine,
                                                  uint8_t *buffer,
                                                  size_t capacity,
                                                  size_t *out_length);

StillResult still_engine_new_variation(struct StillEngine *engine);

StillResult still_engine_quality_set(struct StillEngine *engine, uint32_t quality);

StillResult still_engine_display_refresh_set(struct StillEngine *engine, float refresh_hz);

StillResult still_engine_power_state_set(struct StillEngine *engine,
                                         uint32_t low_power,
                                         uint32_t thermal_level);

StillResult still_engine_preset_select(struct StillEngine *engine, const char *preset_id);

StillResult still_engine_user_preset_save(struct StillEngine *engine, const char *name);

StillResult still_engine_user_preset_delete(struct StillEngine *engine, const char *name);

StillResult still_engine_preset_catalog_export(struct StillEngine *engine,
                                               uint8_t *buffer,
                                               size_t capacity,
                                               size_t *out_length);

StillResult still_engine_state_export(struct StillEngine *engine,
                                      uint8_t *buffer,
                                      size_t capacity,
                                      size_t *out_length);

StillResult still_engine_state_import(struct StillEngine *engine,
                                      const uint8_t *buffer,
                                      size_t length);

StillResult still_engine_metrics_export(struct StillEngine *engine,
                                        uint8_t *buffer,
                                        size_t capacity,
                                        size_t *out_length);

StillResult still_engine_error_drain(struct StillEngine *engine,
                                     char *buffer,
                                     size_t capacity,
                                     size_t *out_length);

StillResult still_engine_event_drain(struct StillEngine *engine,
                                     char *buffer,
                                     size_t capacity,
                                     size_t *out_length);

#endif  /* STILL_H */
