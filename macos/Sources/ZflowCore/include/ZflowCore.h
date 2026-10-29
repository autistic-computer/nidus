#ifndef ZFLOW_CORE_H
#define ZFLOW_CORE_H
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
typedef struct ZflowApp ZflowApp;
ZflowApp *zflow_app_create(const char *path, char **error);
char *zflow_app_request(ZflowApp *app, const char *request);
void zflow_string_free(char *value);
void zflow_app_destroy(ZflowApp *app);

#define ZFLOW_LENS_CHUNK 256
typedef struct {
  uint32_t state;
  uint32_t width;
  uint32_t height;
  uint64_t frames;
} ZflowLensStatus;
ZflowLensStatus zflow_lens_status(const ZflowApp *app);
char *zflow_lens_error(const ZflowApp *app);
size_t zflow_lens_dirty(const ZflowApp *app, double x, double y, double radius, uint32_t *ids,
                        size_t capacity);
bool zflow_lens_chunk(const ZflowApp *app, uint32_t id, uint8_t *pixels, size_t length,
                      size_t stride, uint32_t *width, uint32_t *height);
void zflow_lens_move(const ZflowApp *app, double x, double y, double radius);
void zflow_lens_button(const ZflowApp *app, uint32_t button, bool pressed);
void zflow_lens_key(const ZflowApp *app, uint16_t keycode, bool pressed);
void zflow_lens_scroll(const ZflowApp *app, double dx, double dy);
void zflow_lens_copy(const ZflowApp *app);
void zflow_lens_close(const ZflowApp *app);
#endif
