export const ApiRoutes = {
  activity: '/api/activity',
  providers: '/api/providers',
  collect: '/api/collect',
} as const

export const TauriCommands = {
  getActivity: 'get_activity',
  getProviders: 'get_providers',
  collect: 'collect',
  getProviderConfig: 'get_provider_config',
  saveProviderConfig: 'save_provider_config',
  saveExport: 'save_export',
} as const
