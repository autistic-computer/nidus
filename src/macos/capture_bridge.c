#include <ApplicationServices/ApplicationServices.h>
#include <CoreFoundation/CoreFoundation.h>
#include <dlfcn.h>
#include <math.h>
#include <pthread.h>
#include <stdatomic.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <time.h>
#include <unistd.h>

#define ZFLOW_QUEUE_CAPACITY 1024
#define ZFLOW_MAX_CONTACTS 5
#define ZFLOW_MAX_DISPLAYS 64
#define ZFLOW_CAPS_LOCK 57
#define ZFLOW_KEYBOARD_ISO 0x49534F20

#define ZFLOW_SWALLOWED_EVENTS                                              \
  (CGEventMaskBit(NX_SYSDEFINED) | CGEventMaskBit(18) | CGEventMaskBit(19) | \
   CGEventMaskBit(20) | CGEventMaskBit(29) | CGEventMaskBit(30) |           \
   CGEventMaskBit(31) | CGEventMaskBit(32) | CGEventMaskBit(33) |           \
   CGEventMaskBit(34))

extern Boolean IsSecureEventInputEnabled(void);
extern OSType KBGetLayoutType(SInt16 keyboard_type);

typedef struct { double x, y; } ZFlowMacPosition;
typedef struct { double x, y, width, height; } ZFlowMacRect;

static bool rect_contains(ZFlowMacRect rect, ZFlowMacPosition point) {
  return point.x >= rect.x && point.x < rect.x + rect.width &&
         point.y >= rect.y && point.y < rect.y + rect.height;
}

int zflow_mac_accessibility_authorized(int prompt) {
  if (!prompt) return AXIsProcessTrusted() ? 1 : 0;
  const void *keys[] = {kAXTrustedCheckOptionPrompt};
  const void *values[] = {kCFBooleanTrue};
  CFDictionaryRef options = CFDictionaryCreate(
      NULL, keys, values, 1, &kCFTypeDictionaryKeyCallBacks,
      &kCFTypeDictionaryValueCallBacks);
  if (!options) return 0;
  bool trusted = AXIsProcessTrustedWithOptions(options);
  CFRelease(options);
  return trusted ? 1 : 0;
}

static CGEventRef pass_event(CGEventTapProxy proxy, CGEventType type,
                             CGEventRef event, void *context) {
  (void)proxy;
  (void)type;
  (void)context;
  return event;
}

int zflow_mac_event_tap_allowed(void) {
  CFMachPortRef tap = CGEventTapCreate(
      kCGHIDEventTap, kCGTailAppendEventTap, kCGEventTapOptionDefault,
      CGEventMaskBit(kCGEventOtherMouseDown), pass_event, NULL);
  if (!tap) return 0;
  CGEventTapEnable(tap, false);
  CFMachPortInvalidate(tap);
  CFRelease(tap);
  return 1;
}

int zflow_mac_cursor_position(ZFlowMacPosition *position) {
  if (!position) return -1;
  CGEventRef event = CGEventCreate(NULL);
  if (!event) return -1;
  CGPoint point = CGEventGetLocation(event);
  CFRelease(event);
  if (!isfinite(point.x) || !isfinite(point.y)) return -1;
  *position = (ZFlowMacPosition){point.x, point.y};
  return 0;
}

int zflow_mac_desktop_rectangles(ZFlowMacRect *rectangles, uint32_t capacity) {
  CGDirectDisplayID displays[ZFLOW_MAX_DISPLAYS];
  uint32_t count = 0;
  if (!rectangles || capacity < ZFLOW_MAX_DISPLAYS ||
      CGGetActiveDisplayList(ZFLOW_MAX_DISPLAYS, displays, &count) != kCGErrorSuccess ||
      count == 0 || count > ZFLOW_MAX_DISPLAYS) return -1;
  for (uint32_t i = 0; i < count; i++) {
    CGRect bounds = CGDisplayBounds(displays[i]);
    if (!isfinite(bounds.origin.x) || !isfinite(bounds.origin.y) ||
        !isfinite(bounds.size.width) || !isfinite(bounds.size.height) ||
        bounds.size.width <= 0 || bounds.size.height <= 0) return -1;
    rectangles[i] = (ZFlowMacRect){bounds.origin.x, bounds.origin.y,
                                  bounds.size.width, bounds.size.height};
  }
  return (int)count;
}

static _Atomic uint32_t g_display_generation;

static void displays_changed(CGDirectDisplayID display,
                             CGDisplayChangeSummaryFlags flags, void *context) {
  (void)display;
  (void)flags;
  (void)context;
  atomic_fetch_add(&g_display_generation, 1);
}

static void watch_displays(void) {
  CGDisplayRegisterReconfigurationCallback(displays_changed, NULL);
}

uint32_t zflow_mac_display_generation(void) {
  static pthread_once_t once = PTHREAD_ONCE_INIT;
  pthread_once(&once, watch_displays);
  return atomic_load(&g_display_generation);
}

int zflow_mac_secure_input_enabled(void) {
  return IsSecureEventInputEnabled() ? 1 : 0;
}

int zflow_mac_input_is_neutral(void) {
  for (CGMouseButton button = 0; button < 32; button++)
    if (CGEventSourceButtonState(kCGEventSourceStateHIDSystemState, button)) return 0;
  CGEventFlags held = kCGEventFlagMaskShift | kCGEventFlagMaskControl |
      kCGEventFlagMaskAlternate | kCGEventFlagMaskCommand | kCGEventFlagMaskSecondaryFn;
  return (CGEventSourceFlagsState(kCGEventSourceStateHIDSystemState) & held) == 0;
}

#define ZFLOW_POSTED_MARK 0x7A666C6F77LL

typedef struct {
  double dx, dy;
  uint64_t age_ns;
} ZFlowMacMotion;

static pthread_mutex_t g_motion_lock = PTHREAD_MUTEX_INITIALIZER;
static ZFlowMacMotion g_motion;
static bool g_motion_pending;
static uint64_t g_motion_at;
static pthread_mutex_t g_motion_watch_lock = PTHREAD_MUTEX_INITIALIZER;
static _Atomic bool g_motion_watching;
static CFMachPortRef g_motion_tap;

#define ZFLOW_MAX_EDGE_GUARDS 256
#define ZFLOW_EDGE_GUARD_NS 500000000ULL
typedef struct {
  ZFlowMacRect bounds;
  double start, end;
  uint32_t edge;
} ZFlowMacEdgeGuard;
static ZFlowMacEdgeGuard g_edge_guards[ZFLOW_MAX_EDGE_GUARDS];
static uint32_t g_edge_guard_count, g_edge_guard_generation;
static uint64_t g_edge_guard_until;
static ZFlowMacPosition g_motion_position;
static bool g_motion_position_valid;

void zflow_mac_edge_guards(const ZFlowMacEdgeGuard *guards, uint32_t count) {
  ZFlowMacPosition cursor;
  bool located = zflow_mac_cursor_position(&cursor) == 0;
  pthread_mutex_lock(&g_motion_lock);
  g_edge_guard_count = guards && count <= ZFLOW_MAX_EDGE_GUARDS ? count : 0;
  if (g_edge_guard_count) memcpy(g_edge_guards, guards, count * sizeof(*guards));
  g_edge_guard_until = clock_gettime_nsec_np(CLOCK_UPTIME_RAW) + ZFLOW_EDGE_GUARD_NS;
  g_edge_guard_generation = zflow_mac_display_generation();
  if (!g_motion_position_valid && located) {
    g_motion_position = cursor;
    g_motion_position_valid = true;
  }
  pthread_mutex_unlock(&g_motion_lock);
}

static bool guarded_move(ZFlowMacPosition from, ZFlowMacPosition to,
                         ZFlowMacPosition *held) {
  double first = 2.0;
  for (uint32_t i = 0; i < g_edge_guard_count; i++) {
    ZFlowMacEdgeGuard guard = g_edge_guards[i];
    if (!rect_contains(guard.bounds, from)) continue;
    bool vertical = guard.edge < 2;
    bool negative = guard.edge == 0 || guard.edge == 2;
    double line = vertical ? guard.bounds.x : guard.bounds.y;
    if (!negative) line += (vertical ? guard.bounds.width : guard.bounds.height) - 1;
    double before = vertical ? from.x : from.y, after = vertical ? to.x : to.y;
    if (negative ? !(after < before && before >= line && after <= line)
                 : !(after > before && before <= line && after >= line)) continue;
    double t = before == line ? 1.0 : (line - before) / (after - before);
    double along = vertical ? from.y + t * (to.y - from.y) : from.x + t * (to.x - from.x);
    if (t >= first || along < guard.start || along >= guard.end) continue;
    first = t;
    *held = vertical ? (ZFlowMacPosition){line, along} : (ZFlowMacPosition){along, line};
  }
  return first <= 1.0;
}

static void shorten_warp_freeze(void);

static CGEventRef motion_callback(CGEventTapProxy proxy, CGEventType type,
                                  CGEventRef event, void *context) {
  (void)proxy;
  (void)context;
  if (type == kCGEventTapDisabledByTimeout || type == kCGEventTapDisabledByUserInput) {
    pthread_mutex_lock(&g_motion_lock);
    g_edge_guard_count = 0;
    g_motion_position_valid = false;
    pthread_mutex_unlock(&g_motion_lock);
    if (g_motion_tap) CGEventTapEnable(g_motion_tap, true);
    return event;
  }
  if (type != kCGEventMouseMoved && type != kCGEventLeftMouseDragged &&
      type != kCGEventRightMouseDragged && type != kCGEventOtherMouseDragged) return event;
  CGPoint location = CGEventGetLocation(event);
  ZFlowMacPosition position = {location.x, location.y};
  bool own = CGEventGetIntegerValueField(event, kCGEventSourceUserData) != ZFLOW_POSTED_MARK &&
      CGEventGetIntegerValueField(event, kCGEventSourceUnixProcessID) != getpid();
  uint64_t now = clock_gettime_nsec_np(CLOCK_UPTIME_RAW);
  pthread_mutex_lock(&g_motion_lock);
  if (own) {
    if (!g_motion_pending) g_motion.dx = g_motion.dy = 0;
    g_motion.dx += CGEventGetDoubleValueField(event, kCGMouseEventDeltaX);
    g_motion.dy += CGEventGetDoubleValueField(event, kCGMouseEventDeltaY);
    g_motion_at = now;
    g_motion_pending = true;
  }
  ZFlowMacPosition held;
  bool blocked = own && type == kCGEventMouseMoved && g_edge_guard_count &&
      g_motion_position_valid && now < g_edge_guard_until &&
      g_edge_guard_generation == atomic_load(&g_display_generation) &&
      zflow_mac_input_is_neutral() && !IsSecureEventInputEnabled() &&
      guarded_move(g_motion_position, position, &held);
  if (blocked) {
    static pthread_once_t warp_freeze = PTHREAD_ONCE_INIT;
    pthread_once(&warp_freeze, shorten_warp_freeze);
    blocked = CGWarpMouseCursorPosition(CGPointMake(held.x, held.y)) == kCGErrorSuccess;
    if (blocked) position = held;
  }
  g_motion_position = position;
  g_motion_position_valid = isfinite(position.x) && isfinite(position.y);
  pthread_mutex_unlock(&g_motion_lock);
  return blocked ? NULL : event;
}

int zflow_mac_motion_take(ZFlowMacMotion *motion) {
  if (!motion) return 0;
  pthread_mutex_lock(&g_motion_lock);
  bool pending = g_motion_pending;
  if (pending) {
    *motion = g_motion;
    motion->age_ns = clock_gettime_nsec_np(CLOCK_UPTIME_RAW) - g_motion_at;
  }
  g_motion_pending = false;
  pthread_mutex_unlock(&g_motion_lock);
  return pending ? 1 : 0;
}

static void *motion_thread(void *context) {
  CFRunLoopSourceRef source = context;
  pthread_set_qos_class_self_np(QOS_CLASS_USER_INTERACTIVE, 0);
  CFRunLoopAddSource(CFRunLoopGetCurrent(), source, kCFRunLoopCommonModes);
  CGEventTapEnable(g_motion_tap, true);
  while (CFRunLoopRunInMode(kCFRunLoopDefaultMode, 1.0, false) != kCFRunLoopRunFinished) {
  }
  CFMachPortInvalidate(g_motion_tap);
  CFRelease(source);
  CFRelease(g_motion_tap);
  g_motion_tap = NULL;
  pthread_mutex_lock(&g_motion_lock);
  g_edge_guard_count = 0;
  g_motion_position_valid = false;
  pthread_mutex_unlock(&g_motion_lock);
  atomic_store(&g_motion_watching, false);
  return NULL;
}

int zflow_mac_motion_watch(void) {
  if (atomic_load(&g_motion_watching)) return 0;
  pthread_mutex_lock(&g_motion_watch_lock);
  if (atomic_load(&g_motion_watching)) {
    pthread_mutex_unlock(&g_motion_watch_lock);
    return 0;
  }
  CFMachPortRef tap = CGEventTapCreate(
      kCGHIDEventTap, kCGTailAppendEventTap, kCGEventTapOptionDefault,
      CGEventMaskBit(kCGEventMouseMoved) | CGEventMaskBit(kCGEventLeftMouseDragged) |
      CGEventMaskBit(kCGEventRightMouseDragged) | CGEventMaskBit(kCGEventOtherMouseDragged),
      motion_callback, NULL);
  CFRunLoopSourceRef source = tap ? CFMachPortCreateRunLoopSource(NULL, tap, 0) : NULL;
  g_motion_tap = tap;
  atomic_store(&g_motion_watching, true);
  pthread_t thread;
  int status = source && pthread_create(&thread, NULL, motion_thread, source) == 0 ? 0 : -1;
  if (status == 0) {
    pthread_detach(thread);
  } else {
    atomic_store(&g_motion_watching, false);
    if (source) CFRelease(source);
    if (tap) {
      CFMachPortInvalidate(tap);
      CFRelease(tap);
    }
    g_motion_tap = NULL;
  }
  pthread_mutex_unlock(&g_motion_watch_lock);
  return status;
}

typedef struct { float x, y; } MTPoint;
typedef struct { MTPoint pos, vel; } MTReadout;
typedef struct {
  int frame;
  double timestamp;
  int identifier;
  int state;
  int finger_id;
  int hand_id;
  MTReadout normalized;
  float size;
  int zero1;
  float angle;
  float major_axis;
  float minor_axis;
  MTReadout absolute_vector;
  int zero2[2];
  float z_density;
} MTTouch;

typedef void *MTDeviceRef;
typedef int (*MTContactCallback)(MTDeviceRef, MTTouch *, int, double, int);

typedef struct {
  int32_t id;
  float x;
  float y;
} ZFlowMacContact;

typedef struct {
  uint32_t kind;
  int64_t dx;
  int64_t dy;
  int64_t scroll_x;
  int64_t scroll_y;
  uint16_t code;
  uint16_t button;
  uint8_t pressed;
  uint8_t contact_count;
  uint8_t padding[2];
  int32_t surface_width;
  int32_t surface_height;
  ZFlowMacContact contacts[ZFLOW_MAX_CONTACTS];
} ZFlowMacEvent;

enum {
  ZFLOW_EVENT_MOTION = 1,
  ZFLOW_EVENT_KEY = 2,
  ZFLOW_EVENT_BUTTON = 3,
  ZFLOW_EVENT_TOUCH = 4,
  ZFLOW_EVENT_ESCAPE = 5,
};

static ZFlowMacEvent g_queue[ZFLOW_QUEUE_CAPACITY];
static size_t g_queue_head;
static size_t g_queue_tail;
static pthread_mutex_t g_queue_lock = PTHREAD_MUTEX_INITIALIZER;
static pthread_mutex_t g_run_loop_lock = PTHREAD_MUTEX_INITIALIZER;
static pthread_mutex_t g_start_lock = PTHREAD_MUTEX_INITIALIZER;
static pthread_cond_t g_start_condition = PTHREAD_COND_INITIALIZER;
static pthread_t g_thread;
static bool g_thread_valid;
static bool g_start_ready;
static int g_start_status;
static bool g_request_raw_touch;
static ZFlowMacRect g_entry_region;
static void (*g_wake)(void);
static bool g_forwarded_keys[128];
static uint16_t g_forwarded_codes[128];
static bool g_forwarded_buttons[33];
static _Atomic bool g_stop;
static _Atomic bool g_pause_requested;
static _Atomic bool g_raw_contact_active;
static int g_capture_status;
static char g_error[256] = "no diagnostic";
static CFRunLoopRef g_run_loop;
static bool g_return_pending;
static ZFlowMacPosition g_return_position;
static CFMachPortRef g_event_tap;
static bool g_cursor_hidden;
static bool g_cursor_disconnected;
static bool g_cursor_background;
static int g_cursor_connection;
static CGError (*g_set_connection_property)(int, int, CFStringRef, CFTypeRef);

static void *g_multitouch;
static CFMutableArrayRef g_device_list;
static MTDeviceRef g_raw_devices[8];
static int32_t g_raw_surfaces[8][2];
static size_t g_raw_device_count;
static CFMutableArrayRef (*MTDeviceCreateList)(void);
static void (*MTRegisterContactFrameCallback)(MTDeviceRef, MTContactCallback);
static void (*MTUnregisterContactFrameCallback)(MTDeviceRef, MTContactCallback);
static int (*MTDeviceStart)(MTDeviceRef, int);
static int (*MTDeviceStop)(MTDeviceRef);
static bool (*MTDeviceIsBuiltIn)(MTDeviceRef);
static int (*MTDeviceGetSensorSurfaceDimensions)(MTDeviceRef, int32_t *, int32_t *);

static void set_error(const char *message) {
  snprintf(g_error, sizeof(g_error), "%s", message ? message : "unknown error");
}

static bool cursor_result(CGError error, const char *operation) {
  if (error == kCGErrorSuccess) return true;
  snprintf(g_error, sizeof(g_error), "%s (CGError %d)", operation, error);
  return false;
}

static bool release_cursor(void) {
  bool ok = true;
  if (g_cursor_hidden) {
    if (cursor_result(CGDisplayShowCursor(kCGNullDirectDisplay),
                      "could not show the Mac cursor"))
      g_cursor_hidden = false;
    else
      ok = false;
  }
  if (g_cursor_disconnected) {
    if (cursor_result(CGAssociateMouseAndMouseCursorPosition(true),
                      "could not reconnect the Mac cursor"))
      g_cursor_disconnected = false;
    else
      ok = false;
  }
  return ok;
}

static bool take_return_position(ZFlowMacPosition *position) {
  pthread_mutex_lock(&g_run_loop_lock);
  g_run_loop = NULL;
  bool pending = g_return_pending;
  if (pending) *position = g_return_position;
  g_return_pending = false;
  pthread_mutex_unlock(&g_run_loop_lock);
  return pending;
}

static bool release_cursor_at(const ZFlowMacPosition *position) {
  bool ok = true;
  if (position) {
    ZFlowMacRect displays[ZFLOW_MAX_DISPLAYS];
    int count = zflow_mac_desktop_rectangles(displays, ZFLOW_MAX_DISPLAYS);
    bool on_display = false;
    for (int i = 0; i < count; i++)
      if (rect_contains(displays[i], *position)) on_display = true;
    if (!g_cursor_disconnected || !isfinite(position->x) ||
        !isfinite(position->y) || !on_display) {
      set_error("could not return cursor: capture ended or return point is outside the active displays");
      ok = false;
    } else if (!cursor_result(CGWarpMouseCursorPosition(
                   CGPointMake(position->x, position->y)),
                   "could not place the returning Mac cursor")) {
      ok = false;
    }
  }
  if (!release_cursor()) ok = false;
  return ok;
}

static void shorten_warp_freeze(void) {
  CGEventSourceRef source = CGEventSourceCreate(kCGEventSourceStateCombinedSessionState);
  if (source) CGEventSourceSetLocalEventsSuppressionInterval(source, 0.05);
}

static bool capture_cursor(void) {
  static pthread_once_t warp_freeze = PTHREAD_ONCE_INIT;
  pthread_once(&warp_freeze, shorten_warp_freeze);
  if (g_cursor_hidden || g_cursor_disconnected) {
    set_error("previous capture did not release cursor control");
    return false;
  }
  int (*default_connection)(void) = dlsym(RTLD_DEFAULT, "_CGSDefaultConnection");
  g_set_connection_property = dlsym(RTLD_DEFAULT, "CGSSetConnectionProperty");
  if (!default_connection || !g_set_connection_property) {
    set_error("macOS background cursor control is unavailable");
    return false;
  }
  g_cursor_connection = default_connection();
  if (!g_cursor_background) {
    if (!cursor_result(g_set_connection_property(
            g_cursor_connection, g_cursor_connection,
            CFSTR("SetsCursorInBackground"), kCFBooleanTrue),
            "could not enable background cursor control")) return false;
    g_cursor_background = true;
  }
  if (!cursor_result(CGDisplayHideCursor(kCGNullDirectDisplay),
                     "could not hide the Mac cursor")) return false;
  g_cursor_hidden = true;
  g_cursor_disconnected = true;
  return cursor_result(CGAssociateMouseAndMouseCursorPosition(false),
                       "could not disconnect the Mac cursor");
}

static int capture_entry_allowed(void) {
  if (IsSecureEventInputEnabled()) {
    set_error("secure keyboard entry is on in a Mac app (a password field or "
              "Terminal's Secure Keyboard Entry); input stays on the Mac");
    return -2;
  }
  double right = g_entry_region.x + g_entry_region.width;
  double bottom = g_entry_region.y + g_entry_region.height;
  if (!isfinite(g_entry_region.x) || !isfinite(g_entry_region.y) ||
      !isfinite(g_entry_region.width) || !isfinite(g_entry_region.height) ||
      g_entry_region.width <= 0 || g_entry_region.height <= 0 ||
      !isfinite(right) || !isfinite(bottom) ||
      right <= g_entry_region.x || bottom <= g_entry_region.y) {
    set_error("the configured crossing edge is invalid");
    return -1;
  }
  ZFlowMacPosition current;
  if (zflow_mac_cursor_position(&current) != 0) {
    set_error("could not read the Mac cursor position");
    return -1;
  }
  if (!zflow_mac_input_is_neutral()) {
    set_error("release held buttons and modifier keys before crossing to the other computer");
    return -2;
  }
  if (!rect_contains(g_entry_region, current)) {
    set_error("the Mac cursor left the configured crossing edge");
    return -2;
  }
  return 0;
}

static bool owns_transition(bool *held, bool pressed) {
  if (!pressed && !*held) return false;
  *held = pressed;
  return true;
}

static uint16_t physical_keycode(CGEventRef event, uint16_t keycode) {
  if (keycode != 10 && keycode != 50) return keycode;
  SInt16 keyboard = (SInt16)CGEventGetIntegerValueField(
      event, kCGKeyboardEventKeyboardType);
  return KBGetLayoutType(keyboard) == ZFLOW_KEYBOARD_ISO ? 60 - keycode : keycode;
}

static void wake(void) {
  if (g_wake) g_wake();
}

static bool enqueue(const ZFlowMacEvent *event) {
  pthread_mutex_lock(&g_queue_lock);
  size_t next = (g_queue_head + 1) % ZFLOW_QUEUE_CAPACITY;
  if (next == g_queue_tail) {
    pthread_mutex_unlock(&g_queue_lock);
    return false;
  }
  g_queue[g_queue_head] = *event;
  g_queue_head = next;
  pthread_mutex_unlock(&g_queue_lock);
  wake();
  return true;
}

static void clear_capture_queue(void) {
  pthread_mutex_lock(&g_queue_lock);
  g_queue_head = 0;
  g_queue_tail = 0;
  pthread_mutex_unlock(&g_queue_lock);
}

static int contact_callback(MTDeviceRef device, MTTouch *touches, int count,
                            double timestamp, int frame) {
  (void)timestamp;
  (void)frame;
  ZFlowMacEvent event = {0};
  event.kind = ZFLOW_EVENT_TOUCH;
  for (size_t i = 0; i < sizeof(g_raw_devices) / sizeof(g_raw_devices[0]); i++) {
    if (g_raw_devices[i] == device) {
      event.surface_width = g_raw_surfaces[i][0];
      event.surface_height = g_raw_surfaces[i][1];
      break;
    }
  }
  for (int i = 0; i < count && event.contact_count < ZFLOW_MAX_CONTACTS; i++) {
    if (touches[i].state != 3 && touches[i].state != 4) continue;

    atomic_store(&g_raw_contact_active, true);

    float x = touches[i].normalized.pos.x;
    float y = touches[i].normalized.pos.y;
    if (!isfinite(x) || !isfinite(y)) return 0;

    ZFlowMacContact *contact = &event.contacts[event.contact_count++];
    contact->id = touches[i].identifier;
    contact->x = fminf(fmaxf(x, 0.0f), 1.0f);
    contact->y = fminf(fmaxf(y, 0.0f), 1.0f);
  }
  atomic_store(&g_raw_contact_active, event.contact_count > 0);
  enqueue(&event);
  return 0;
}

static uint8_t side_modifier_pressed(CGEventFlags flags,
                                     CGEventFlags aggregate_mask,
                                     CGEventFlags side_mask,
                                     CGEventFlags left_mask,
                                     CGEventFlags right_mask) {
  CGEventFlags device_mask = left_mask | right_mask;
  if ((flags & device_mask) != 0) return (flags & side_mask) != 0;
  return (flags & aggregate_mask) != 0;
}

uint8_t zflow_mac_modifier_pressed(uint16_t keycode, uint64_t raw_flags) {
  CGEventFlags flags = (CGEventFlags)raw_flags;
  switch (keycode) {
    case 54: return side_modifier_pressed(
        flags, kCGEventFlagMaskCommand, NX_DEVICERCMDKEYMASK,
        NX_DEVICELCMDKEYMASK, NX_DEVICERCMDKEYMASK);
    case 55: return side_modifier_pressed(
        flags, kCGEventFlagMaskCommand, NX_DEVICELCMDKEYMASK,
        NX_DEVICELCMDKEYMASK, NX_DEVICERCMDKEYMASK);
    case 56: return side_modifier_pressed(
        flags, kCGEventFlagMaskShift, NX_DEVICELSHIFTKEYMASK,
        NX_DEVICELSHIFTKEYMASK, NX_DEVICERSHIFTKEYMASK);
    case 60: return side_modifier_pressed(
        flags, kCGEventFlagMaskShift, NX_DEVICERSHIFTKEYMASK,
        NX_DEVICELSHIFTKEYMASK, NX_DEVICERSHIFTKEYMASK);
    case 58: return side_modifier_pressed(
        flags, kCGEventFlagMaskAlternate, NX_DEVICELALTKEYMASK,
        NX_DEVICELALTKEYMASK, NX_DEVICERALTKEYMASK);
    case 61: return side_modifier_pressed(
        flags, kCGEventFlagMaskAlternate, NX_DEVICERALTKEYMASK,
        NX_DEVICELALTKEYMASK, NX_DEVICERALTKEYMASK);
    case 59: return side_modifier_pressed(
        flags, kCGEventFlagMaskControl, NX_DEVICELCTLKEYMASK,
        NX_DEVICELCTLKEYMASK, NX_DEVICERCTLKEYMASK);
    case 62: return side_modifier_pressed(
        flags, kCGEventFlagMaskControl, NX_DEVICERCTLKEYMASK,
        NX_DEVICELCTLKEYMASK, NX_DEVICERCTLKEYMASK);
    default: return 0;
  }
}

static void stop_capture_run_loop(void) {
  pthread_mutex_lock(&g_run_loop_lock);
  if (g_run_loop) CFRunLoopStop(g_run_loop);
  pthread_mutex_unlock(&g_run_loop_lock);
}

static void end_capture(const char *error) {
  if (error) {
    set_error(error);
    g_capture_status = -1;
  }
  atomic_store(&g_stop, true);
  stop_capture_run_loop();
  wake();
}

static CGEventRef forward(const ZFlowMacEvent *event) {
  if (!enqueue(event))
    end_capture("the capture queue overflowed; input returned to the Mac");
  return NULL;
}

static void release_lifted_input(void) {
  ZFlowMacEvent release = {.kind = ZFLOW_EVENT_KEY};
  for (uint16_t key = 0; key < 128; key++) {
    if (!g_forwarded_keys[key] ||
        CGEventSourceKeyState(kCGEventSourceStateHIDSystemState, key)) continue;
    g_forwarded_keys[key] = false;
    release.code = g_forwarded_codes[key];
    forward(&release);
  }
  release = (ZFlowMacEvent){.kind = ZFLOW_EVENT_BUTTON};
  for (uint16_t button = 1; button < 33; button++) {
    if (!g_forwarded_buttons[button] ||
        CGEventSourceButtonState(kCGEventSourceStateHIDSystemState,
                                 (CGMouseButton)(button - 1))) continue;
    g_forwarded_buttons[button] = false;
    release.button = button;
    forward(&release);
  }
}

uint8_t zflow_mac_should_forward_scroll(uint8_t raw_touch,
                                        uint8_t raw_contact_active,
                                        int64_t scroll_phase,
                                        int64_t momentum_phase) {
  return !raw_touch ||
      (!raw_contact_active && scroll_phase == 0 && momentum_phase == 0);
}

static CGEventRef event_callback(CGEventTapProxy proxy, CGEventType type,
                                 CGEventRef event, void *context) {
  (void)proxy;
  (void)context;
  if (type == kCGEventTapDisabledByTimeout) {
    if (!zflow_mac_event_tap_allowed()) {
      end_capture("Accessibility access was removed; ending remote control");
      return event;
    }
    if (g_event_tap) CGEventTapEnable(g_event_tap, true);
    release_lifted_input();
    return event;
  }
  if (type == kCGEventTapDisabledByUserInput) {
    end_capture("macOS disabled input capture; ending remote control");
    return event;
  }
  if (atomic_load(&g_stop)) return event;
  if ((uint32_t)type < 64 && (ZFLOW_SWALLOWED_EVENTS & CGEventMaskBit(type)))
    return NULL;

  ZFlowMacEvent captured = {0};
  switch (type) {
    case kCGEventKeyDown:
    case kCGEventKeyUp:
    case kCGEventFlagsChanged: {
      uint16_t keycode = (uint16_t)CGEventGetIntegerValueField(
          event, kCGKeyboardEventKeycode);
      CGEventFlags flags = CGEventGetFlags(event);
      if (type == kCGEventKeyDown && keycode == 51 &&
          (flags & kCGEventFlagMaskControl) &&
          (flags & kCGEventFlagMaskCommand)) {
        captured.kind = ZFLOW_EVENT_ESCAPE;
        atomic_store(&g_pause_requested, true);
        enqueue(&captured);
        end_capture(NULL);
        return NULL;
      }
      captured.kind = ZFLOW_EVENT_KEY;
      captured.code = keycode;
      if (type == kCGEventFlagsChanged && keycode == ZFLOW_CAPS_LOCK) {
        captured.pressed = 1;
        forward(&captured);
        captured.pressed = 0;
        return forward(&captured);
      }
      captured.pressed = type == kCGEventFlagsChanged
          ? zflow_mac_modifier_pressed(keycode, flags)
          : type == kCGEventKeyDown;
      if (type == kCGEventKeyDown && keycode < 128 && !g_forwarded_keys[keycode] &&
          CGEventGetIntegerValueField(event, kCGKeyboardEventAutorepeat)) return NULL;
      if (keycode < 128) {
        bool held = g_forwarded_keys[keycode];
        if (!owns_transition(&g_forwarded_keys[keycode], captured.pressed)) return event;
        if (!held) g_forwarded_codes[keycode] = physical_keycode(event, keycode);
        captured.code = g_forwarded_codes[keycode];
      }
      return forward(&captured);
    }
    case kCGEventLeftMouseDown:
    case kCGEventRightMouseDown:
    case kCGEventOtherMouseDown:
    case kCGEventLeftMouseUp:
    case kCGEventRightMouseUp:
    case kCGEventOtherMouseUp:
      captured.kind = ZFLOW_EVENT_BUTTON;
      captured.button = (uint16_t)CGEventGetIntegerValueField(
          event, kCGMouseEventButtonNumber) + 1;
      captured.pressed = type == kCGEventLeftMouseDown ||
                         type == kCGEventRightMouseDown ||
                         type == kCGEventOtherMouseDown;
      if (captured.button < 33 &&
          !owns_transition(&g_forwarded_buttons[captured.button], captured.pressed)) return event;
      return forward(&captured);
    case kCGEventMouseMoved:
    case kCGEventLeftMouseDragged:
    case kCGEventRightMouseDragged:
    case kCGEventOtherMouseDragged:
      if (!g_request_raw_touch || !atomic_load(&g_raw_contact_active)) {
        captured.kind = ZFLOW_EVENT_MOTION;
        captured.dx = CGEventGetIntegerValueField(event, kCGMouseEventDeltaX);
        captured.dy = CGEventGetIntegerValueField(event, kCGMouseEventDeltaY);
        return forward(&captured);
      }
      return NULL;
    case kCGEventScrollWheel:
      if (zflow_mac_should_forward_scroll(
              g_request_raw_touch, atomic_load(&g_raw_contact_active),
              CGEventGetIntegerValueField(event, kCGScrollWheelEventScrollPhase),
              CGEventGetIntegerValueField(event, kCGScrollWheelEventMomentumPhase))) {
        captured.kind = ZFLOW_EVENT_MOTION;
        captured.scroll_x = -CGEventGetIntegerValueField(
            event, kCGScrollWheelEventPointDeltaAxis2);
        captured.scroll_y = CGEventGetIntegerValueField(
            event, kCGScrollWheelEventPointDeltaAxis1);
        return forward(&captured);
      }
      return NULL;
    default:
      return event;
  }
}

static void unload_multitouch(void) {
  for (size_t i = 0; i < g_raw_device_count; i++) {
    if (MTUnregisterContactFrameCallback)
      MTUnregisterContactFrameCallback(g_raw_devices[i], contact_callback);
    if (MTDeviceStop) MTDeviceStop(g_raw_devices[i]);
  }
  g_raw_device_count = 0;
  if (g_device_list) CFRelease(g_device_list);
  g_device_list = NULL;
  if (g_multitouch) dlclose(g_multitouch);
  g_multitouch = NULL;
  atomic_store(&g_raw_contact_active, false);
}

static bool load_multitouch(void) {
  g_multitouch = dlopen(
      "/System/Library/PrivateFrameworks/MultitouchSupport.framework/MultitouchSupport",
      RTLD_NOW);
  if (!g_multitouch) {
    set_error(dlerror());
    return false;
  }
  MTDeviceCreateList = dlsym(g_multitouch, "MTDeviceCreateList");
  MTRegisterContactFrameCallback =
      dlsym(g_multitouch, "MTRegisterContactFrameCallback");
  MTUnregisterContactFrameCallback =
      dlsym(g_multitouch, "MTUnregisterContactFrameCallback");
  MTDeviceStart = dlsym(g_multitouch, "MTDeviceStart");
  MTDeviceStop = dlsym(g_multitouch, "MTDeviceStop");
  MTDeviceIsBuiltIn = dlsym(g_multitouch, "MTDeviceIsBuiltIn");
  MTDeviceGetSensorSurfaceDimensions =
      dlsym(g_multitouch, "MTDeviceGetSensorSurfaceDimensions");
  if (!MTDeviceCreateList || !MTRegisterContactFrameCallback ||
      !MTDeviceStart || !MTDeviceIsBuiltIn) {
    set_error("MultitouchSupport is missing required symbols");
    unload_multitouch();
    return false;
  }

  g_device_list = MTDeviceCreateList();
  CFIndex count = g_device_list ? CFArrayGetCount(g_device_list) : 0;
  bool found = false;
  for (CFIndex i = 0; i < count && g_raw_device_count < 8; i++) {
    MTDeviceRef device = (MTDeviceRef)CFArrayGetValueAtIndex(g_device_list, i);
    if (MTDeviceIsBuiltIn(device)) continue;
    found = true;
    int32_t *surface = g_raw_surfaces[g_raw_device_count];
    if (!MTDeviceGetSensorSurfaceDimensions ||
        MTDeviceGetSensorSurfaceDimensions(device, &surface[0], &surface[1]) != 0)
      surface[0] = surface[1] = 0;
    g_raw_devices[g_raw_device_count++] = device;
    MTRegisterContactFrameCallback(device, contact_callback);
    if (MTDeviceStart(device, 0) != 0) {
      set_error("Magic Trackpad failed to start");
      unload_multitouch();
      return false;
    }
  }
  if (!found) {
    set_error("no external Magic Trackpad was enumerated");
    unload_multitouch();
    return false;
  }
  return true;
}

static void signal_started(int status) {
  pthread_mutex_lock(&g_start_lock);
  g_start_status = status;
  g_start_ready = true;
  pthread_cond_signal(&g_start_condition);
  pthread_mutex_unlock(&g_start_lock);
}

static void *capture_thread(void *context) {
  (void)context;
  pthread_set_qos_class_self_np(QOS_CLASS_USER_INTERACTIVE, 0);
  bool raw_active = g_request_raw_touch && load_multitouch();
  if (!raw_active) g_request_raw_touch = false;

  CGEventMask mask = CGEventMaskBit(kCGEventKeyDown) |
      CGEventMaskBit(kCGEventKeyUp) |
      CGEventMaskBit(kCGEventFlagsChanged) |
      CGEventMaskBit(kCGEventLeftMouseDown) |
      CGEventMaskBit(kCGEventLeftMouseUp) |
      CGEventMaskBit(kCGEventRightMouseDown) |
      CGEventMaskBit(kCGEventRightMouseUp) |
      CGEventMaskBit(kCGEventOtherMouseDown) |
      CGEventMaskBit(kCGEventOtherMouseUp) |
      CGEventMaskBit(kCGEventMouseMoved) |
      CGEventMaskBit(kCGEventLeftMouseDragged) |
      CGEventMaskBit(kCGEventRightMouseDragged) |
      CGEventMaskBit(kCGEventOtherMouseDragged) |
      CGEventMaskBit(kCGEventScrollWheel) |
      ZFLOW_SWALLOWED_EVENTS;
  g_event_tap = CGEventTapCreate(
      kCGHIDEventTap, kCGHeadInsertEventTap, kCGEventTapOptionDefault,
      mask, event_callback, NULL);
  if (!g_event_tap) {
    set_error("CGEventTapCreate failed; Accessibility permission is required");
    unload_multitouch();
    signal_started(-1);
    return NULL;
  }

  CFRunLoopSourceRef source = CFMachPortCreateRunLoopSource(NULL, g_event_tap, 0);
  if (!source) {
    set_error("could not create the macOS capture run-loop source");
    CFMachPortInvalidate(g_event_tap);
    CFRelease(g_event_tap);
    g_event_tap = NULL;
    unload_multitouch();
    signal_started(-1);
    return NULL;
  }
  CFRunLoopRef run_loop = CFRunLoopGetCurrent();
  pthread_mutex_lock(&g_run_loop_lock);
  g_run_loop = run_loop;
  pthread_mutex_unlock(&g_run_loop_lock);
  CFRunLoopAddSource(run_loop, source, kCFRunLoopCommonModes);
  CGEventTapEnable(g_event_tap, true);
  int admission = capture_entry_allowed();
  bool cursor_active = admission == 0 && capture_cursor();
  if (cursor_active) {
    clear_capture_queue();
    signal_started(raw_active ? 1 : 0);
    while (!atomic_load(&g_stop)) {
      if (CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.1, false) ==
          kCFRunLoopRunFinished) {
        set_error("macOS stopped input capture; input returned to the Mac");
        g_capture_status = -1;
        break;
      }
    }
  }

  ZFlowMacPosition return_position;
  bool returning = take_return_position(&return_position);
  CGEventTapEnable(g_event_tap, false);
  CFRunLoopRemoveSource(run_loop, source, kCFRunLoopCommonModes);
  CFMachPortInvalidate(g_event_tap);
  CFRelease(source);
  CFRelease(g_event_tap);
  g_event_tap = NULL;
  if (!release_cursor_at(returning ? &return_position : NULL)) g_capture_status = -1;
  unload_multitouch();
  atomic_store(&g_stop, true);
  wake();
  if (!cursor_active) signal_started(admission == -2 && g_capture_status == 0 ? -2 : -1);
  return NULL;
}

int zflow_mac_capture_start(int raw_touch, const ZFlowMacRect *entry,
                            void (*wake)(void)) {
  if (g_thread_valid) {
    set_error("capture is already running");
    return -1;
  }
  if (!entry) {
    set_error("the configured crossing edge is invalid");
    return -1;
  }
  clear_capture_queue();
  atomic_store(&g_stop, false);
  atomic_store(&g_pause_requested, false);
  atomic_store(&g_raw_contact_active, false);
  g_capture_status = 0;
  set_error("no diagnostic");
  pthread_mutex_lock(&g_run_loop_lock);
  g_run_loop = NULL;
  g_return_pending = false;
  pthread_mutex_unlock(&g_run_loop_lock);
  g_request_raw_touch = raw_touch != 0;
  g_entry_region = *entry;
  g_wake = wake;
  memset(g_forwarded_keys, 0, sizeof(g_forwarded_keys));
  memset(g_forwarded_buttons, 0, sizeof(g_forwarded_buttons));
  g_start_ready = false;
  g_start_status = -1;
  if (pthread_create(&g_thread, NULL, capture_thread, NULL) != 0) {
    set_error("could not create the macOS capture thread");
    return -1;
  }
  g_thread_valid = true;
  pthread_mutex_lock(&g_start_lock);
  while (!g_start_ready) pthread_cond_wait(&g_start_condition, &g_start_lock);
  int status = g_start_status;
  pthread_mutex_unlock(&g_start_lock);
  if (status < 0) {
    pthread_join(g_thread, NULL);
    g_thread_valid = false;
  }
  return status;
}

int zflow_mac_capture_poll(ZFlowMacEvent *event) {
  if (!event || pthread_mutex_trylock(&g_queue_lock) != 0) return 0;
  if (g_queue_tail == g_queue_head) {
    pthread_mutex_unlock(&g_queue_lock);
    return 0;
  }
  *event = g_queue[g_queue_tail];
  g_queue_tail = (g_queue_tail + 1) % ZFLOW_QUEUE_CAPACITY;
  pthread_mutex_unlock(&g_queue_lock);
  return 1;
}

int zflow_mac_capture_stop_requested(void) {
  return atomic_load(&g_stop) ? 1 : 0;
}

int zflow_mac_capture_pause_requested(void) {
  return atomic_load(&g_pause_requested) ? 1 : 0;
}

int zflow_mac_capture_stop_at(const ZFlowMacPosition *position) {
  const char *request_error = NULL;
  if (!g_thread_valid) {
    if (position) {
      set_error("could not return cursor: capture already ended");
      return -1;
    }
    return g_capture_status;
  }
  pthread_mutex_lock(&g_run_loop_lock);
  if (position) {
    if (!isfinite(position->x) || !isfinite(position->y)) {
      request_error = "could not return cursor: return point is not finite";
    } else if (!g_run_loop || atomic_load(&g_stop)) {
      request_error = "could not return cursor: capture already ended";
    } else {
      g_return_position = *position;
      g_return_pending = true;
    }
  }
  atomic_store(&g_stop, true);
  if (g_run_loop) CFRunLoopStop(g_run_loop);
  pthread_mutex_unlock(&g_run_loop_lock);
  pthread_join(g_thread, NULL);
  g_thread_valid = false;
  atomic_store(&g_raw_contact_active, false);
  if (request_error) {
    set_error(request_error);
    g_capture_status = -1;
  }
  return g_capture_status;
}

int zflow_mac_capture_stop(void) {
  return zflow_mac_capture_stop_at(NULL);
}

const char *zflow_mac_capture_last_error(void) {
  return g_error;
}
