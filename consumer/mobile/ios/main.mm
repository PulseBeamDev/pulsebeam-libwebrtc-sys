#import <UIKit/UIKit.h>
#include <cstdlib>

extern "C" int pulsebeam_headless_run();

@interface QualificationDelegate : UIResponder <UIApplicationDelegate>
@property(nonatomic, strong) UIWindow* window;
@end

@implementation QualificationDelegate
- (BOOL)application:(UIApplication*)application
    didFinishLaunchingWithOptions:(NSDictionary*)options {
  self.window = [[UIWindow alloc] initWithFrame:UIScreen.mainScreen.bounds];
  self.window.rootViewController = [[UIViewController alloc] init];
  [self.window makeKeyAndVisible];
  dispatch_async(dispatch_get_global_queue(QOS_CLASS_USER_INITIATED, 0), ^{
    @autoreleasepool {
      const int status = pulsebeam_headless_run();
      NSDictionary* environment = NSProcessInfo.processInfo.environment;
      NSDictionary* result = @{
        @"pulsebeam_status": @(status),
        @"os_version": UIDevice.currentDevice.systemVersion,
        @"pulsebeam_nonce": environment[@"PULSEBEAM_NONCE"] ?: @"",
        @"pulsebeam_archive_sha256": environment[@"PULSEBEAM_ARCHIVE_SHA256"] ?: @""
      };
      NSData* bytes = [NSJSONSerialization dataWithJSONObject:result options:0 error:nil];
      NSString* path = [NSHomeDirectory() stringByAppendingPathComponent:@"Documents/pulsebeam-result.json"];
      if (![bytes writeToFile:path atomically:YES]) std::exit(2);
      // This is a disposable qualification host, not an application SDK. All
      // Rust peers, factories, channels and SDK threads have already torn down.
      std::exit(status);
    }
  });
  return YES;
}
@end

int main(int argc, char** argv) {
  @autoreleasepool {
    return UIApplicationMain(argc, argv, nil, NSStringFromClass(QualificationDelegate.class));
  }
}
