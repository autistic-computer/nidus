#import <AppKit/AppKit.h>
#include <ImageIO/ImageIO.h>
#include <dispatch/dispatch.h>
#include <pthread.h>
#include <stdatomic.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

enum {
  NID_CLIP_EMPTY = 0,
  NID_CLIP_TEXT = 1,
  NID_CLIP_PNG = 2,
  NID_CLIP_UNCHANGED = 3,
  NID_CLIP_NOT_ALLOWED = 4,
};

enum {
  ACCESS_DEFAULT = 0,
  ACCESS_ASK = 1,
  ACCESS_ALWAYS_ALLOW = 2,
  ACCESS_ALWAYS_DENY = 3,
};

static NSString *const NID_CLIP_MARKER = @"io.nid.clip";

static NSString *const CONCEALED_TYPE = @"org.nspasteboard.ConcealedType";
static NSString *const TRANSIENT_TYPE = @"org.nspasteboard.TransientType";

static double nid_pasteboard_wait = 3.0;

enum { WORK_PENDING, WORK_RUNNING, WORK_ABANDONED };

static bool on_main(void (^work)(void)) {
  if (pthread_main_np()) {
    work();
    return true;
  }
  dispatch_semaphore_t done = dispatch_semaphore_create(0);
  __block _Atomic int state = WORK_PENDING;
  dispatch_async(dispatch_get_main_queue(), ^{
    int pending = WORK_PENDING;
    if (!atomic_compare_exchange_strong(&state, &pending, WORK_RUNNING)) return;
    work();
    dispatch_semaphore_signal(done);
  });
  int64_t wait = (int64_t)(nid_pasteboard_wait * (double)NSEC_PER_SEC);
  if (dispatch_semaphore_wait(done, dispatch_time(DISPATCH_TIME_NOW, wait)) == 0) return true;
  int pending = WORK_PENDING;
  if (atomic_compare_exchange_strong(&state, &pending, WORK_ABANDONED)) return false;
  dispatch_semaphore_wait(done, DISPATCH_TIME_FOREVER);
  return true;
}

static NSPasteboard *pasteboard_named(NSString *name) {
  return name ? [NSPasteboard pasteboardWithName:name] : NSPasteboard.generalPasteboard;
}

static NSData *png_from_tiff(NSData *tiff) {
  CGImageSourceRef source = CGImageSourceCreateWithData((__bridge CFDataRef)tiff, NULL);
  if (!source) return nil;
  NSMutableData *png = [NSMutableData data];
  CGImageDestinationRef destination =
      CGImageDestinationCreateWithData((__bridge CFMutableDataRef)png, CFSTR("public.png"), 1, NULL);
  bool converted = false;
  if (destination && CGImageSourceGetCount(source) > 0) {
    CGImageDestinationAddImageFromSource(destination, source, 0, NULL);
    converted = CGImageDestinationFinalize(destination);
  }
  if (destination) CFRelease(destination);
  CFRelease(source);
  return converted ? png : nil;
}

static NSInteger access_behavior(NSPasteboard *board) {
  if (@available(macOS 15.4, *)) return board.accessBehavior;
  return ACCESS_ALWAYS_ALLOW;
}

static NSInteger (*nid_access_behavior)(NSPasteboard *) = access_behavior;

static bool may_read(NSInteger behavior) {
  return behavior == ACCESS_ALWAYS_ALLOW || behavior == ACCESS_DEFAULT;
}

static NSData *clip_on(NSPasteboard *board, uint32_t *kind, bool *tiff) {
  NSArray<NSPasteboardType> *types = board.types;
  if ([types containsObject:CONCEALED_TYPE] || [types containsObject:TRANSIENT_TYPE]) return nil;
  NSString *text = [board stringForType:NSPasteboardTypeString];
  NSData *found = nil;
  if (text) {
    found = [text dataUsingEncoding:NSUTF8StringEncoding];
    *kind = NID_CLIP_TEXT;
  } else if ((found = [board dataForType:NSPasteboardTypePNG])) {
    *kind = NID_CLIP_PNG;
  } else if ((found = [board dataForType:NSPasteboardTypeTIFF])) {
    *kind = NID_CLIP_PNG;
    *tiff = true;
  }
  return found;
}

int nid_mac_pasteboard_read(const char *name, const int64_t *unchanged_at, size_t limit,
                              uint32_t *kind, uint8_t **data, size_t *length,
                              int64_t *change_count) {
  if (!kind || !data || !length || !change_count) return -1;
  *kind = NID_CLIP_EMPTY;
  *data = NULL;
  *length = 0;
  @autoreleasepool {
    NSString *board_name = name ? [NSString stringWithUTF8String:name] : nil;
    bool skip = unchanged_at != NULL;
    int64_t unchanged = skip ? *unchanged_at : 0;
    __block NSInteger count = 0;
    __block NSData *found = nil;
    __block uint32_t found_kind = NID_CLIP_EMPTY;
    __block bool tiff = false;
    bool answered = on_main(^{
      NSPasteboard *board = pasteboard_named(board_name);
      count = board.changeCount;
      if (skip && count == unchanged) {
        found_kind = NID_CLIP_UNCHANGED;
        return;
      }
      if (!may_read(nid_access_behavior(board))) {
        found_kind = NID_CLIP_NOT_ALLOWED;
        return;
      }
      found = clip_on(board, &found_kind, &tiff);
    });
    if (!answered) return -1;
    *change_count = count;
    *kind = found_kind;
    if (!found) return 0;
    if (tiff && !(found = png_from_tiff(found))) return -1;
    *length = found.length;
    if (found.length == 0 || found.length > limit) return 0;
    *data = malloc(found.length);
    if (!*data) return -1;
    memcpy(*data, found.bytes, found.length);
    return 0;
  }
}

void nid_mac_pasteboard_free(uint8_t *data) {
  free(data);
}

void nid_mac_pasteboard_ask(const char *name) {
  @autoreleasepool {
    NSString *board_name = name ? [NSString stringWithUTF8String:name] : nil;
    dispatch_async(dispatch_get_main_queue(), ^{
      @autoreleasepool {
        NSPasteboard *board = pasteboard_named(board_name);
        if (nid_access_behavior(board) != ACCESS_DEFAULT) return;
        uint32_t kind = NID_CLIP_EMPTY;
        bool tiff = false;
        (void)clip_on(board, &kind, &tiff);
      }
    });
  }
}

int nid_mac_pasteboard_write(const char *name, uint32_t kind, const uint8_t *data, size_t length,
                               int64_t *change_count) {
  if (!data || length == 0 || !change_count) return -1;
  if (kind != NID_CLIP_TEXT && kind != NID_CLIP_PNG) return -1;
  @autoreleasepool {
    NSString *board_name = name ? [NSString stringWithUTF8String:name] : nil;
    NSData *bytes = [NSData dataWithBytes:data length:length];
    NSPasteboardType type = kind == NID_CLIP_TEXT ? NSPasteboardTypeString : NSPasteboardTypePNG;
    __block NSInteger count = 0;
    __block BOOL written = NO;
    bool answered = on_main(^{
      NSPasteboardItem *item = [NSPasteboardItem new];
      if (![item setData:bytes forType:type] ||
          ![item setData:[NSData data] forType:NID_CLIP_MARKER])
        return;
      NSPasteboard *board = pasteboard_named(board_name);
      NSInteger cleared = [board clearContents];
      written = [board writeObjects:@[ item ]] && board.changeCount == cleared;
      count = cleared;
    });
    if (!answered || !written) return -1;
    *change_count = count;
    return 0;
  }
}
