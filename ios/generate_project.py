#!/usr/bin/env python3
"""Generate the small, dependency-free Xcode project. Xcode itself is the build tool."""
from pathlib import Path
import hashlib, json
root = Path(__file__).parent
objects = {}
def uid(name): return hashlib.sha256(name.encode()).hexdigest()[:24].upper()
def add(name, body): objects[uid(name)] = body; return uid(name)
def q(s): return json.dumps(s)
def arr(xs): return '(' + ', '.join(xs) + (',)' if xs else ')')
def config_list(name, settings):
    refs = []
    for config in ['Debug', 'Release']:
        merged = dict(settings)
        merged['SWIFT_OPTIMIZATION_LEVEL'] = '-Onone' if config == 'Debug' else '-O'
        merged['SWIFT_ACTIVE_COMPILATION_CONDITIONS'] = 'DEBUG $(inherited)' if config == 'Debug' else '$(inherited)'
        merged['ENABLE_TESTABILITY'] = 'YES' if config == 'Debug' else 'NO'
        if name == 'FieldCRM': merged['INFOPLIST_FILE'] = 'FieldCRM/Info.Debug.plist' if config == 'Debug' else 'FieldCRM/Info.plist'
        refs.append(add(name+config, 'isa = XCBuildConfiguration; name = '+config+'; buildSettings = {' + ''.join(q(k)+' = '+q(v)+';' for k,v in merged.items()) + '};'))
    return add(name+'configs', 'isa = XCConfigurationList; buildConfigurations = '+arr(refs)+'; defaultConfigurationIsVisible = 0; defaultConfigurationName = Release;')
products=[]; groups=[]; targets=[]
package=add('sqlcipher-package','isa = XCRemoteSwiftPackageReference; repositoryURL = "https://github.com/sqlcipher/SQLCipher.swift.git"; requirement = {kind = exactVersion; version = 4.19.0;};')
product=add('sqlcipher-product','isa = XCSwiftPackageProductDependency; package = '+package+'; productName = SQLCipher;')
for name in ['FieldCRM','FieldCRMTests','FieldCRMUITests']:
    sourcefiles=[]; buildfiles=[]
    for file in sorted((root/name).glob('*.swift')):
        ref=add(str(file.relative_to(root)), 'isa = PBXFileReference; lastKnownFileType = sourcecode.swift; path = '+q(file.name)+'; sourceTree = "<group>";')
        sourcefiles.append(ref); buildfiles.append(add(str(file)+'build','isa = PBXBuildFile; fileRef = '+ref+';'))
    groups.append(add(name+'group','isa = PBXGroup; children = '+arr(sourcefiles)+'; path = '+name+'; sourceTree = "<group>";'))
    ext='app' if name=='FieldCRM' else 'xctest'
    prod=add(name+'product','isa = PBXFileReference; explicitFileType = '+('wrapper.application' if ext=='app' else 'wrapper.cfbundle')+'; path = '+name+'.'+ext+'; sourceTree = BUILT_PRODUCTS_DIR;'); products.append(prod)
    sources=add(name+'sources','isa = PBXSourcesBuildPhase; buildActionMask = 2147483647; files = '+arr(buildfiles)+'; runOnlyForDeploymentPostprocessing = 0;')
    frameworks=[]
    if name!='FieldCRMUITests': frameworks.append(add(name+'sqlcipherlink','isa = PBXBuildFile; productRef = '+product+';'))
    framework=add(name+'frameworks','isa = PBXFrameworksBuildPhase; buildActionMask = 2147483647; files = '+arr(frameworks)+'; runOnlyForDeploymentPostprocessing = 0;')
    resourcefiles=[]
    if name=='FieldCRMTests':
        ref=add('fixtures','isa = PBXFileReference; lastKnownFileType = folder; path = ../mobile/contracts; sourceTree = SOURCE_ROOT;')
        resourcefiles.append(add('fixturesbuild','isa = PBXBuildFile; fileRef = '+ref+';'))
    resources=add(name+'resources','isa = PBXResourcesBuildPhase; buildActionMask = 2147483647; files = '+arr(resourcefiles)+'; runOnlyForDeploymentPostprocessing = 0;')
    settings={'PRODUCT_NAME':'$(TARGET_NAME)','PRODUCT_BUNDLE_IDENTIFIER':'dev.crm.'+name,'SWIFT_VERSION':'5.0','IPHONEOS_DEPLOYMENT_TARGET':'17.0','TARGETED_DEVICE_FAMILY':'1,2','SDKROOT':'iphoneos','SUPPORTED_PLATFORMS':'iphoneos iphonesimulator','CODE_SIGN_STYLE':'Automatic','DEVELOPMENT_TEAM':'','OTHER_SWIFT_FLAGS':'$(inherited) -Xcc -DSQLITE_HAS_CODEC=1','GCC_PREPROCESSOR_DEFINITIONS':'$(inherited) SQLITE_HAS_CODEC=1','GENERATE_INFOPLIST_FILE':'YES','LD_RUNPATH_SEARCH_PATHS':'$(inherited) @executable_path/Frameworks @loader_path/Frameworks','SWIFT_EMIT_LOC_STRINGS':'NO'}
    dependencies=[]
    if name=='FieldCRM': settings.update({'GENERATE_INFOPLIST_FILE':'NO','CURRENT_PROJECT_VERSION':'1','MARKETING_VERSION':'0.1.0'})
    else:
        dep=add(name+'dependency','isa = PBXTargetDependency; target = '+uid('FieldCRMtarget')+';'); dependencies=[dep]
        settings['TEST_TARGET_NAME']='FieldCRM'
        if name=='FieldCRMTests': settings.update({'TEST_HOST':'$(BUILT_PRODUCTS_DIR)/FieldCRM.app/$(BUNDLE_EXECUTABLE_FOLDER_PATH)/FieldCRM','BUNDLE_LOADER':'$(TEST_HOST)'})
    config=config_list(name,settings)
    typ='application' if name=='FieldCRM' else ('bundle.unit-test' if name=='FieldCRMTests' else 'bundle.ui-testing')
    targets.append(add(name+'target','isa = PBXNativeTarget; buildConfigurationList = '+config+'; buildPhases = '+arr([sources,framework,resources])+'; buildRules = (); dependencies = '+arr(dependencies)+'; name = '+name+'; productName = '+name+'; productReference = '+prod+'; productType = "com.apple.product-type.'+typ+'"; packageProductDependencies = '+arr([product] if name!='FieldCRMUITests' else [])+';'))
pg=add('products','isa = PBXGroup; children = '+arr(products)+'; name = Products; sourceTree = "<group>";')
main=add('main','isa = PBXGroup; children = '+arr(groups+[pg])+'; sourceTree = "<group>";')
config=config_list('project',{'CLANG_ENABLE_MODULES':'YES','CLANG_ENABLE_OBJC_ARC':'YES','ENABLE_STRICT_OBJC_MSGSEND':'YES','DEBUG_INFORMATION_FORMAT':'dwarf-with-dsym'})
project=add('project','isa = PBXProject; attributes = {BuildIndependentTargetsInParallel = YES; LastUpgradeCheck = 2660;}; buildConfigurationList = '+config+'; compatibilityVersion = "Xcode 14.0"; developmentRegion = en; knownRegions = (en,Base); mainGroup = '+main+'; productRefGroup = '+pg+'; projectDirPath = ""; projectRoot = ""; targets = '+arr(targets)+'; packageReferences = '+arr([package])+';')
(root/'FieldCRM.xcodeproj/project.pbxproj').write_text('// !$*UTF8*$!\n{archiveVersion = 1; classes = {}; objectVersion = 56; objects = {\n'+''.join(k+' = {'+v+'};\n' for k,v in objects.items())+'}; rootObject = '+project+';}\n')
def ref(name): return f'<BuildableReference BuildableIdentifier="primary" BlueprintIdentifier="{uid(name+"target")}" BuildableName="{name}.{ "app" if name=="FieldCRM" else "xctest"}" BlueprintName="{name}" ReferencedContainer="container:FieldCRM.xcodeproj"/>'
scheme=f'''<?xml version="1.0" encoding="UTF-8"?><Scheme LastUpgradeVersion="2660" version="1.3"><BuildAction parallelizeBuildables="YES" buildImplicitDependencies="YES"><BuildActionEntries><BuildActionEntry buildForTesting="YES" buildForRunning="YES" buildForProfiling="YES" buildForArchiving="YES" buildForAnalyzing="YES">{ref('FieldCRM')}</BuildActionEntry></BuildActionEntries></BuildAction><TestAction buildConfiguration="Debug" selectedDebuggerIdentifier="Xcode.DebuggerFoundation.Debugger.LLDB" selectedLauncherIdentifier="Xcode.IDEFoundation.Launcher.LLDB" shouldUseLaunchSchemeArgsEnv="YES"><Testables><TestableReference skipped="NO">{ref('FieldCRMTests')}</TestableReference><TestableReference skipped="NO">{ref('FieldCRMUITests')}</TestableReference></Testables></TestAction><LaunchAction buildConfiguration="Debug" selectedDebuggerIdentifier="Xcode.DebuggerFoundation.Debugger.LLDB" selectedLauncherIdentifier="Xcode.IDEFoundation.Launcher.LLDB" launchStyle="0" useCustomWorkingDirectory="NO" ignoresPersistentStateOnLaunch="NO" debugDocumentVersioning="YES" allowLocationSimulation="YES"><BuildableProductRunnable runnableDebuggingMode="0">{ref('FieldCRM')}</BuildableProductRunnable></LaunchAction><ProfileAction buildConfiguration="Release" shouldUseLaunchSchemeArgsEnv="YES" savedToolIdentifier="" useCustomWorkingDirectory="NO" debugDocumentVersioning="YES"><BuildableProductRunnable runnableDebuggingMode="0">{ref('FieldCRM')}</BuildableProductRunnable></ProfileAction><AnalyzeAction buildConfiguration="Debug"/><ArchiveAction buildConfiguration="Release" revealArchiveInOrganizer="YES"/></Scheme>'''
(root/'FieldCRM.xcodeproj/xcshareddata/xcschemes/FieldCRM.xcscheme').write_text(scheme)
