import type { ReactNode } from 'react';

import type { ValidationIssue } from '../types/mesh.js';

interface FieldShellProps {
  children: ReactNode;
  hint?: string | undefined;
  issue: ValidationIssue | undefined;
  label: string;
  unit?: string | undefined;
}

interface NumberFieldProps extends Omit<FieldShellProps, 'children'> {
  integer?: boolean;
  max?: number;
  min?: number;
  onChange(value: number): void;
  step?: number | 'any';
  value: number;
}

interface TextFieldProps extends Omit<FieldShellProps, 'children'> {
  maxLength?: number;
  onChange(value: string): void;
  value: string;
}

interface SelectFieldProps<T extends string>
  extends Omit<FieldShellProps, 'children'> {
  onChange(value: T): void;
  options: readonly { label: string; value: T }[];
  value: T;
}

export function NumberField({
  hint,
  integer = false,
  issue,
  label,
  max,
  min,
  onChange,
  step = 'any',
  unit,
  value,
}: NumberFieldProps): ReactNode {
  return (
    <FieldShell hint={hint} issue={issue} label={label} unit={unit}>
      <input
        aria-invalid={issue?.severity === 'ERROR'}
        inputMode={integer ? 'numeric' : 'decimal'}
        max={max}
        min={min}
        onChange={(event) => {
          const parsed = Number(event.currentTarget.value);
          onChange(Number.isNaN(parsed) ? 0 : parsed);
        }}
        step={step}
        type="number"
        value={Number.isFinite(value) ? value : ''}
      />
    </FieldShell>
  );
}

export function TextField({
  hint,
  issue,
  label,
  maxLength,
  onChange,
  unit,
  value,
}: TextFieldProps): ReactNode {
  return (
    <FieldShell hint={hint} issue={issue} label={label} unit={unit}>
      <input
        aria-invalid={issue?.severity === 'ERROR'}
        maxLength={maxLength}
        onChange={(event) => {
          onChange(event.currentTarget.value);
        }}
        spellCheck={false}
        type="text"
        value={value}
      />
    </FieldShell>
  );
}

export function SelectField<T extends string>({
  hint,
  issue,
  label,
  onChange,
  options,
  unit,
  value,
}: SelectFieldProps<T>): ReactNode {
  return (
    <FieldShell hint={hint} issue={issue} label={label} unit={unit}>
      <select
        aria-invalid={issue?.severity === 'ERROR'}
        onChange={(event) => {
          onChange(event.currentTarget.value as T);
        }}
        value={value}
      >
        {options.map((option) => (
          <option key={option.value} value={option.value}>
            {option.label}
          </option>
        ))}
      </select>
    </FieldShell>
  );
}

export function Toggle({
  checked,
  description,
  label,
  onChange,
}: {
  checked: boolean;
  description?: string;
  label: string;
  onChange(value: boolean): void;
}): ReactNode {
  return (
    <label className="toggle-row">
      <span>
        <strong>{label}</strong>
        {description === undefined ? null : <small>{description}</small>}
      </span>
      <input
        checked={checked}
        onChange={(event) => {
          onChange(event.currentTarget.checked);
        }}
        type="checkbox"
      />
      <span aria-hidden="true" className="toggle-track">
        <span />
      </span>
    </label>
  );
}

export function SectionTitle({
  children,
  description,
}: {
  children: ReactNode;
  description: string;
}): ReactNode {
  return (
    <div className="section-title">
      <div>
        <h2>{children}</h2>
        <p>{description}</p>
      </div>
    </div>
  );
}

function FieldShell({
  children,
  hint,
  issue,
  label,
  unit,
}: FieldShellProps): ReactNode {
  return (
    <label className={`field ${issue === undefined ? '' : 'field--issue'}`}>
      <span className="field__label">
        {label}
        {unit === undefined ? null : <em>{unit}</em>}
      </span>
      {children}
      {issue === undefined ? (
        hint === undefined ? null : <small>{hint}</small>
      ) : (
        <small className={`field__${issue.severity.toLowerCase()}`}>
          {issue.message}
        </small>
      )}
    </label>
  );
}
