#import <WebRTC/WebRTC.h>

#include <cstdio>
#include <cstdlib>

static void VerifyPlatformH264Factories() {
  RTCDefaultVideoEncoderFactory *encoder_factory =
      [[RTCDefaultVideoEncoderFactory alloc] init];
  RTCDefaultVideoDecoderFactory *decoder_factory =
      [[RTCDefaultVideoDecoderFactory alloc] init];
  RTCVideoCodecInfo *encoder_h264 = nil;
  RTCVideoCodecInfo *decoder_h264 = nil;
  for (RTCVideoCodecInfo *codec in encoder_factory.supportedCodecs) {
    if ([codec.name caseInsensitiveCompare:@"H264"] == NSOrderedSame) {
      encoder_h264 = codec;
      break;
    }
  }
  for (RTCVideoCodecInfo *codec in decoder_factory.supportedCodecs) {
    if ([codec.name caseInsensitiveCompare:@"H264"] == NSOrderedSame) {
      decoder_h264 = codec;
      break;
    }
  }
  if (encoder_h264 == nil || decoder_h264 == nil ||
      [encoder_factory createEncoder:encoder_h264] == nil ||
      [decoder_factory createDecoder:decoder_h264] == nil) {
    std::fprintf(stderr,
                 "FAIL upstream VideoToolbox H.264 formats or adapters\n");
    std::_Exit(1);
  }
}

#if TARGET_OS_IPHONE
#import <UIKit/UIKit.h>

@interface NativePackageProbeDelegate : UIResponder <UIApplicationDelegate>
@property(nonatomic, strong) UIWindow *window;
@end

@implementation NativePackageProbeDelegate
- (BOOL)application:(UIApplication *)application
    didFinishLaunchingWithOptions:(NSDictionary *)options {
  RTCPeerConnectionFactory *factory = [[RTCPeerConnectionFactory alloc] init];
  NSArray *cameras = [RTCCameraVideoCapturer captureDevices];
  RTCAudioSession *audio = [RTCAudioSession sharedInstance];
  VerifyPlatformH264Factories();
  if (factory == nil || cameras == nil || audio == nil) {
    std::fprintf(stderr, "FAIL native iOS factories\n");
    std::_Exit(1);
  }
  std::fprintf(stderr, "PASS native iOS factories loaded\n");
  std::_Exit(0);
}
@end

int main(int argc, char **argv) {
  @autoreleasepool {
    return UIApplicationMain(
        argc, argv, nil, NSStringFromClass([NativePackageProbeDelegate class]));
  }
}
#else
int main() {
  @autoreleasepool {
    RTCPeerConnectionFactory *factory = [[RTCPeerConnectionFactory alloc] init];
    NSArray *cameras = [RTCCameraVideoCapturer captureDevices];
    RTCDesktopMediaList *desktop =
        [[RTCDesktopMediaList alloc] initWithType:RTCDesktopSourceTypeScreen
                                         delegate:nil];
    VerifyPlatformH264Factories();
    if (factory == nil || cameras == nil || desktop == nil) {
      std::fprintf(stderr, "FAIL native macOS factories\n");
      return 1;
    }
    std::fprintf(stderr, "PASS native macOS factories loaded\n");
  }
  return 0;
}
#endif
