import { inject, provide, type InjectionKey } from 'vue'
import * as original from '../../api/activityImports'
import * as admitted from '../../api/admittedActivityImports'

type ActivityReaders = Pick<typeof original,
  'fetchActivityMappings' | 'fetchActivityRecords' | 'fetchActivityResults' |
  'fetchActivityTargets' | 'fetchActivityObservations' | 'fetchActivityField' | 'useActivityAccess'>
const family: InjectionKey<ActivityReaders> = Symbol('activity readers')

export function provideAdmittedActivityReaders() { provide(family, admitted) }
export function useActivityReaders(explicitFamily?: 'admitted'): ActivityReaders { return explicitFamily === 'admitted' ? admitted : inject(family, original) }
