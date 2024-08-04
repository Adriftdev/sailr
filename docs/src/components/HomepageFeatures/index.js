import clsx from 'clsx';
import Heading from '@theme/Heading';
import styles from './styles.module.css';

const FeatureList = [
  {
    title: 'Automated Deployments',
    Svg: require('@site/static/img/automation.png').default,
    description: (
      <>
        Automate deployments and updates so you can sail through your work.
      </>
    ),
  },
  {
    title: 'Resource Management',
    Svg: require('@site/static/img/resource.png').default,
    description: (
      <>
        Manage resources efficiently so you don't run aground.
      </>
    ),
  },
  {
    title: 'Opinionated',
    Svg: require('@site/static/img/opinionated.png').default,
    description: (
      <>
        Opinioned kubernetes infrastructure automation, So you don't have to think about it; Because that's a thing no one should do too often.. Think.
      </>
    ),
  },
];

function Feature({ Svg, title, description }) {
  return (
    <div className={clsx('col col--4')}>
      <div className="text--center">
        <img src={Svg} className={styles.featureSvg} role="img" />
      </div>
      <div className="text--center padding-horiz--md">
        <Heading as="h3">{title}</Heading>
        <p>{description}</p>
      </div>
    </div>
  );
}

export default function HomepageFeatures() {
  return (
    <section className={styles.features}>
      <div className="container">
        <div className="row">
          {FeatureList.map((props, idx) => (
            <Feature key={idx} {...props} />
          ))}
        </div>
      </div>
    </section>
  );
}
