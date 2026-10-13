import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useReducer,
  type ReactNode,
} from 'react';

import {
  errorMessage,
  generatePreview,
  initializeMeshEngine,
  validateWithEngine,
} from '../api/meshEngine.js';
import {
  createDefaultConfiguration,
  DEFAULT_CONFIGURATION,
} from '../config/defaultConfiguration.js';
import type {
  MeshConfiguration,
  MeshPreview,
  PreviewLevel,
  ValidationReport,
} from '../types/mesh.js';
import { synchronizeReynoldsNumber } from '../utils/flow.js';
import { validateConfiguration } from '../validation/configValidation.js';

type EngineStatus = 'LOADING' | 'READY' | 'GENERATING' | 'ERROR';

interface WorkspaceState {
  configuration: MeshConfiguration;
  report: ValidationReport;
  preview: MeshPreview | undefined;
  previewLevel: PreviewLevel;
  engineStatus: EngineStatus;
  engineError: string | undefined;
  dirty: boolean;
}

type Action =
  | { type: 'ENGINE_READY' }
  | { type: 'ENGINE_ERROR'; message: string }
  | { type: 'GENERATION_STARTED' }
  | { type: 'GENERATION_FINISHED'; preview: MeshPreview }
  | { type: 'CONFIGURATION_CHANGED'; configuration: MeshConfiguration }
  | { type: 'CONFIGURATION_LOADED'; configuration: MeshConfiguration }
  | { type: 'PREVIEW_LEVEL_CHANGED'; level: PreviewLevel }
  | { type: 'REPORT_REPLACED'; report: ValidationReport };

interface MeshWorkspaceValue extends WorkspaceState {
  updateConfiguration(configuration: MeshConfiguration): void;
  loadConfiguration(configuration: MeshConfiguration): Promise<void>;
  resetConfiguration(): void;
  setPreviewLevel(level: PreviewLevel): void;
  generate(): Promise<void>;
}

const initialConfiguration = createDefaultConfiguration();
const initialState: WorkspaceState = {
  configuration: initialConfiguration,
  report: validateConfiguration(initialConfiguration),
  preview: undefined,
  previewLevel: 'COARSE_MESH',
  engineStatus: 'LOADING',
  engineError: undefined,
  dirty: true,
};

const MeshWorkspaceContext = createContext<MeshWorkspaceValue | undefined>(
  undefined,
);

export function MeshWorkspaceProvider({
  children,
}: {
  children: ReactNode;
}): ReactNode {
  const [state, dispatch] = useReducer(reducer, initialState);

  useEffect(() => {
    let active = true;
    void initializeMeshEngine()
      .then(async () => {
        if (!active) {
          return;
        }
        dispatch({ type: 'ENGINE_READY' });
        const preview = await generatePreview(
          DEFAULT_CONFIGURATION,
          'COARSE_MESH',
        );
        if (active) {
          dispatch({ type: 'GENERATION_FINISHED', preview });
        }
      })
      .catch((error: unknown) => {
        if (active) {
          dispatch({ type: 'ENGINE_ERROR', message: errorMessage(error) });
        }
      });
    return () => {
      active = false;
    };
  }, []);

  const updateConfiguration = useCallback(
    (configuration: MeshConfiguration) => {
      dispatch({
        type: 'CONFIGURATION_CHANGED',
        configuration: synchronizeReynoldsNumber(configuration),
      });
    },
    [],
  );

  const loadConfiguration = useCallback(
    async (configuration: MeshConfiguration) => {
      const synchronized = synchronizeReynoldsNumber(configuration);
      const report = await validateWithEngine(synchronized);
      dispatch({ type: 'CONFIGURATION_LOADED', configuration: synchronized });
      dispatch({ type: 'REPORT_REPLACED', report });
    },
    [],
  );

  const resetConfiguration = useCallback(() => {
    dispatch({
      type: 'CONFIGURATION_LOADED',
      configuration: createDefaultConfiguration(),
    });
  }, []);

  const setPreviewLevel = useCallback((level: PreviewLevel) => {
    dispatch({ type: 'PREVIEW_LEVEL_CHANGED', level });
  }, []);

  const generate = useCallback(async () => {
    if (!state.report.valid || state.engineStatus === 'GENERATING') {
      return;
    }
    dispatch({ type: 'GENERATION_STARTED' });
    try {
      const report = await validateWithEngine(state.configuration);
      dispatch({ type: 'REPORT_REPLACED', report });
      if (!report.valid) {
        dispatch({
          type: 'ENGINE_ERROR',
          message: 'Resolve the configuration errors before generating.',
        });
        return;
      }
      const preview = await generatePreview(
        state.configuration,
        state.previewLevel,
      );
      dispatch({ type: 'GENERATION_FINISHED', preview });
    } catch (error: unknown) {
      dispatch({ type: 'ENGINE_ERROR', message: errorMessage(error) });
    }
  }, [
    state.configuration,
    state.engineStatus,
    state.previewLevel,
    state.report.valid,
  ]);

  const value = useMemo<MeshWorkspaceValue>(
    () => ({
      ...state,
      updateConfiguration,
      loadConfiguration,
      resetConfiguration,
      setPreviewLevel,
      generate,
    }),
    [
      generate,
      loadConfiguration,
      resetConfiguration,
      setPreviewLevel,
      state,
      updateConfiguration,
    ],
  );

  return (
    <MeshWorkspaceContext.Provider value={value}>
      {children}
    </MeshWorkspaceContext.Provider>
  );
}

export function useMeshWorkspace(): MeshWorkspaceValue {
  const value = useContext(MeshWorkspaceContext);
  if (value === undefined) {
    throw new Error('useMeshWorkspace must be used inside MeshWorkspaceProvider');
  }
  return value;
}

function reducer(state: WorkspaceState, action: Action): WorkspaceState {
  switch (action.type) {
    case 'ENGINE_READY':
      return { ...state, engineStatus: 'READY', engineError: undefined };
    case 'ENGINE_ERROR':
      return {
        ...state,
        engineStatus: 'ERROR',
        engineError: action.message,
      };
    case 'GENERATION_STARTED':
      return {
        ...state,
        engineStatus: 'GENERATING',
        engineError: undefined,
      };
    case 'GENERATION_FINISHED':
      return {
        ...state,
        engineStatus: 'READY',
        engineError: undefined,
        preview: action.preview,
        dirty: false,
      };
    case 'CONFIGURATION_CHANGED':
      return {
        ...state,
        configuration: action.configuration,
        report: validateConfiguration(action.configuration),
        dirty: true,
      };
    case 'CONFIGURATION_LOADED':
      return {
        ...state,
        configuration: action.configuration,
        report: validateConfiguration(action.configuration),
        dirty: true,
        engineError: undefined,
      };
    case 'PREVIEW_LEVEL_CHANGED':
      return { ...state, previewLevel: action.level, dirty: true };
    case 'REPORT_REPLACED':
      return { ...state, report: action.report };
  }
}
