// The app the integration suite runs in a session's simulator: a label and
// a button whose title changes when it is pressed. Built by
// tests/integration with the simulator SDK's clang; see there.
//
// It also reports what it is and what it can reach, when asked to through
// its environment (simctl passes on variables named SIMCTL_CHILD_...):
//   SV2_PROBE_REPORT   the file to write the report to
//   SV2_PROBE_HOME     a home directory to try to list
//   SV2_PROBE_VICTIM   a directory to try to list and to write a file in
// The suite then reads the report, and looks for the file itself.
#import <UIKit/UIKit.h>
#include <unistd.h>

static void report(void) {
    NSDictionary *env = NSProcessInfo.processInfo.environment;
    NSString *out = env[@"SV2_PROBE_REPORT"];
    if (out == nil) {
        return;
    }
    NSFileManager *files = NSFileManager.defaultManager;
    NSString *home = env[@"SV2_PROBE_HOME"] ?: @"/nonexistent";
    NSString *victim = env[@"SV2_PROBE_VICTIM"] ?: @"/nonexistent";
    NSString *marker = [victim stringByAppendingPathComponent:@"written-by-the-probe"];
    BOOL wrote = [@"probe\n" writeToFile:marker atomically:NO encoding:NSUTF8StringEncoding error:nil];
    NSString *text = [NSString
        stringWithFormat:@"uid=%d\nlist-home=%@\nlist-victim=%@\nwrite-victim=%@\n", getuid(),
                         [files contentsOfDirectoryAtPath:home error:nil] ? @"yes" : @"no",
                         [files contentsOfDirectoryAtPath:victim error:nil] ? @"yes" : @"no",
                         wrote ? @"yes" : @"no"];
    [text writeToFile:out atomically:NO encoding:NSUTF8StringEncoding error:nil];
}

@interface ProbeViewController : UIViewController
@end

@implementation ProbeViewController
- (void)viewDidLoad {
    [super viewDidLoad];
    self.view.backgroundColor = UIColor.whiteColor;
    UILabel *label = [[UILabel alloc] initWithFrame:CGRectMake(20, 120, 340, 40)];
    label.text = @"Hello from sv2 probe";
    [self.view addSubview:label];
    UIButton *button = [UIButton buttonWithType:UIButtonTypeSystem];
    button.frame = CGRectMake(20, 200, 200, 44);
    [button setTitle:@"Press me" forState:UIControlStateNormal];
    [button addTarget:self action:@selector(pressed:) forControlEvents:UIControlEventTouchUpInside];
    [self.view addSubview:button];
}

- (void)pressed:(UIButton *)button {
    [button setTitle:@"Pressed" forState:UIControlStateNormal];
}
@end

// A scene delegate: an app without one does not start on current runtimes.
@interface ProbeSceneDelegate : UIResponder <UIWindowSceneDelegate>
@property(strong, nonatomic) UIWindow *window;
@end

@implementation ProbeSceneDelegate
- (void)scene:(UIScene *)scene
    willConnectToSession:(UISceneSession *)session
                 options:(UISceneConnectionOptions *)options {
    self.window = [[UIWindow alloc] initWithWindowScene:(UIWindowScene *)scene];
    self.window.rootViewController = [ProbeViewController new];
    [self.window makeKeyAndVisible];
}
@end

@interface ProbeAppDelegate : UIResponder <UIApplicationDelegate>
@end

@implementation ProbeAppDelegate
- (UISceneConfiguration *)application:(UIApplication *)application
    configurationForConnectingSceneSession:(UISceneSession *)session
                                   options:(UISceneConnectionOptions *)options {
    UISceneConfiguration *configuration =
        [[UISceneConfiguration alloc] initWithName:@"Default" sessionRole:session.role];
    configuration.delegateClass = ProbeSceneDelegate.class;
    return configuration;
}
@end

int main(int argc, char **argv) {
    @autoreleasepool {
        report();
        return UIApplicationMain(argc, argv, nil, NSStringFromClass(ProbeAppDelegate.class));
    }
}
